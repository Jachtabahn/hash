#![allow(
    clippy::match_same_arms,
    clippy::cast_possible_wrap,
    clippy::for_kv_map
)]

use std::collections::HashMap;

use super::prelude::*;

use crate::datastore::schema::{
    FieldType, PREVIOUS_INDEX_COLUMN_INDEX, PREVIOUS_INDEX_COLUMN_NAME,
};
use crate::datastore::{
    error::Result,
    prelude::*,
    schema::{FieldSpec, FieldSpecMap, FieldTypeVariant, PresetFieldType},
};

impl PresetFieldType {
    const fn is_fixed_size(&self) -> bool {
        match self {
            PresetFieldType::Index => true,
            PresetFieldType::Id => true,
            PresetFieldType::Arrow(_) => todo!(),
        }
    }

    #[must_use]
    pub const fn get_arrow_data_type(&self) -> ArrowDataType {
        match self {
            PresetFieldType::Index => ArrowDataType::UInt32,
            PresetFieldType::Id => {
                ArrowDataType::FixedSizeBinary(crate::datastore::UUID_V4_LEN as i32)
            }
            PresetFieldType::Arrow(_) => todo!(),
        }
    }
}

impl FieldType {
    fn is_fixed_size(&self) -> bool {
        match &self.variant {
            FieldTypeVariant::Number | FieldTypeVariant::Boolean => true,
            FieldTypeVariant::String | FieldTypeVariant::Serialized => false,
            FieldTypeVariant::FixedLengthArray {
                kind: inner,
                len: _,
            } => inner.is_fixed_size(),
            FieldTypeVariant::VariableLengthArray(_) => false,
            FieldTypeVariant::Struct(inner) => inner.iter().all(FieldSpec::is_fixed_size),
            FieldTypeVariant::Preset(inner) => inner.is_fixed_size(),
        }
    }

    pub fn get_arrow_data_type(&self) -> Result<ArrowDataType> {
        match &self.variant {
            FieldTypeVariant::Number => Ok(ArrowDataType::Float64),
            FieldTypeVariant::Boolean => Ok(ArrowDataType::Boolean),
            FieldTypeVariant::String => Ok(ArrowDataType::Utf8),
            FieldTypeVariant::Serialized => Ok(ArrowDataType::Utf8),
            FieldTypeVariant::FixedLengthArray { kind: inner, len } => Ok(
                ArrowDataType::FixedSizeList(Box::new(inner.get_arrow_data_type()?), *len as i32),
            ),
            FieldTypeVariant::VariableLengthArray(inner) => {
                Ok(ArrowDataType::List(Box::new(inner.get_arrow_data_type()?)))
            }
            FieldTypeVariant::Struct(inner) => Ok(ArrowDataType::Struct(
                inner
                    .iter()
                    .map(FieldSpec::get_arrow_field)
                    .collect::<Result<Vec<_>>>()?,
            )),
            FieldTypeVariant::Preset(inner) => Ok(inner.get_arrow_data_type()),
        }
    }
}

impl FieldSpec {
    fn is_fixed_size(&self) -> bool {
        self.field_type.is_fixed_size()
    }

    pub fn get_arrow_field(&self) -> Result<ArrowField> {
        // This is required because non-nullable user-defined columns
        // are nullable in schemas (not every agent uses that col)
        // while non-nullable built-ins must be nullable
        let base_nullability = if self.is_built_in() {
            self.field_type.nullable
        } else {
            true
        };
        Ok(ArrowField::new(
            &self.name,
            self.field_type.get_arrow_data_type()?,
            base_nullability,
        ))
    }
}

impl FieldSpecMap {
    pub fn get_arrow_schema(&self) -> Result<ArrowSchema> {
        let mut partitioned_keys = Vec::with_capacity(self.len());
        let mut fixed_size_no = 0;

        let mut any_types = vec![];

        for (_key, field_spec) in self.iter() {
            if field_spec.is_fixed_size() {
                partitioned_keys.insert(0, field_spec);
                fixed_size_no += 1;
            } else {
                partitioned_keys.push(field_spec);
            }

            if matches!(field_spec.key_type.variant, FieldTypeVariant::Serialized) {
                any_types.push(field_spec.name.clone())
            }
        }

        // Sort both partitions by key names
        let name_sort = |a: &&FieldSpec, b: &&FieldSpec| a.name.cmp(&b.name);
        partitioned_keys[0..fixed_size_no].sort_by(name_sort);

        // Ensure our special key is in the right place
        if &partitioned_keys[PREVIOUS_INDEX_COLUMN_INDEX].name != PREVIOUS_INDEX_COLUMN_NAME {
            if let Some(cur_index) = partitioned_keys[0..fixed_size_no]
                .iter()
                .position(|b| b.name == PREVIOUS_INDEX_COLUMN_NAME)
            {
                partitioned_keys[0..fixed_size_no].swap(cur_index, PREVIOUS_INDEX_COLUMN_INDEX)
            } else {
                return Err(Error::SpecialKeyMissing(
                    PREVIOUS_INDEX_COLUMN_NAME.to_string(),
                ));
            }
        }
        partitioned_keys[fixed_size_no..].sort_by(name_sort);
        let nullabilities = partitioned_keys
            .iter()
            .map(|key| (key.key_type.nullable as usize).to_string())
            .collect::<Vec<_>>();

        let mut metadata = HashMap::with_capacity(1);
        metadata.insert("serialized".into(), any_types.join(","));
        metadata.insert("nullable".into(), nullabilities.join(","));
        Ok(ArrowSchema::new_with_metadata(
            partitioned_keys
                .iter()
                .map(|k| k.get_arrow_field())
                .collect::<Result<_>>()?,
            metadata,
        ))
    }
}

pub trait IsFixedSize {
    fn is_fixed_size(&self) -> Result<bool>;
}

impl IsFixedSize for ArrowDataType {
    fn is_fixed_size(&self) -> Result<bool> {
        match self {
            ArrowDataType::Float64 => Ok(true),
            ArrowDataType::FixedSizeBinary(_) => Ok(true),
            ArrowDataType::Utf8 => Ok(false),
            ArrowDataType::FixedSizeList(val, _) => val.is_fixed_size(),
            ArrowDataType::List(_) => Ok(false),
            _ => Err(Error::NotImplemented(SupportedType::ArrowDataType(
                self.clone(),
            ))),
        }
    }
}

#[cfg(test)]
pub mod tests {
    // use crate::datastore::schema::FieldType;

    use super::*;
    // use crate::hash_types::state::AgentStateField;

    #[test]
    fn get_schema() -> Result<()> {
        let mut field_spec_map = FieldSpecMap::default()?;
        // TODO OS [3] - RUNTIME BLOCK - Bring in line with accessors
        panic!();
        // field_spec_map
        //     .add(FieldSpec::new_mergeable(
        //         "test1",
        //         FieldType::new(FieldTypeVariant::Boolean, true),
        //     ))
        //     .unwrap();
        // field_spec_map
        //     .add(FieldSpec::new_mergeable(
        //         "test2",
        //         FieldType::new(
        //             FieldTypeVariant::VariableLengthArray(Box::new(FieldType::new(
        //                 FieldTypeVariant::Number,
        //                 false,
        //             ))),
        //             true,
        //         ),
        //     ))
        //     .unwrap();
        // field_spec_map
        //     .add(FieldSpec::new_mergeable(
        //         "test3",
        //         FieldType::new(
        //             FieldTypeVariant::FixedLengthArray {
        //                 kind: Box::new(FieldType::new(FieldTypeVariant::Number, false)),
        //                 len: 3,
        //             },
        //             true,
        //         ),
        //     ))
        //     .unwrap();
        //
        // field_spec_map
        //     .add_built_in(&AgentStateField::AgentId)
        //     .unwrap();
        //
        // let mut meta = HashMap::new();
        // meta.insert("serialized".into(), "".into());
        // meta.insert("nullable".into(), "1,0,1,1,1".into());
        // let target = ArrowSchema::new_with_metadata(
        //     vec![
        //         ArrowField::new(
        //             PREVIOUS_INDEX_COLUMN_NAME,
        //             ArrowDataType::FixedSizeList(Box::new(ArrowDataType::UInt32), 2),
        //             true,
        //         ),
        //         ArrowField::new(
        //             "agent_id",
        //             ArrowDataType::FixedSizeBinary(crate::datastore::UUID_V4_LEN as i32),
        //             false,
        //         ),
        //         ArrowField::new("test1", ArrowDataType::Boolean, true),
        //         ArrowField::new(
        //             "test3",
        //             ArrowDataType::FixedSizeList(Box::new(ArrowDataType::Float64), 3),
        //             true,
        //         ),
        //         ArrowField::new(
        //             "test2",
        //             ArrowDataType::List(Box::new(ArrowDataType::Float64)),
        //             true,
        //         ),
        //     ],
        //     meta,
        // );

        let schema = field_spec_map.get_arrow_schema().unwrap();
        // assert_eq!(schema, target);
        Ok(())
    }
}