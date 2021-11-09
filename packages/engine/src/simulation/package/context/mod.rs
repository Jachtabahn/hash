use std::sync::Arc;

use crate::simulation::Result;
use crate::{
    datastore::{
        meta::ColumnDynamicMetadata,
        prelude::Result as DatastoreResult,
        schema::{FieldSpec, FieldSpecMapBuilder},
        table::state::view::StateSnapshot,
    },
    simulation::comms::package::PackageComms,
    SimRunConfig,
};

use super::{
    deps::Dependencies,
    ext_traits::{GetWorkerStartMsg, MaybeCPUBound},
    prelude::*,
};
pub use crate::config::Globals;
use crate::datastore::schema::accessor::FieldSpecMapAccessor;
use crate::proto::ExperimentRunBase;
pub use packages::{ContextTask, ContextTaskMessage, ContextTaskResult, Name, PACKAGES};

pub mod packages;

#[async_trait]
pub trait Package: MaybeCPUBound + GetWorkerStartMsg + Send + Sync {
    async fn run<'s>(
        &mut self,
        state: Arc<State>,
        snapshot: Arc<StateSnapshot>,
    ) -> Result<ContextColumn>;
    fn get_empty_arrow_column(&self, num_agents: usize) -> Result<Arc<dyn arrow::array::Array>>;
}

pub trait PackageCreator: Sync {
    /// Create the package.
    fn create(
        &self,
        config: &Arc<SimRunConfig<ExperimentRunBase>>,
        system: PackageComms,
        accessor: FieldSpecMapAccessor,
    ) -> Result<Box<dyn Package>>;

    fn get_dependencies(&self) -> Result<Dependencies> {
        Ok(Dependencies::empty())
    }

    fn add_context_field_specs(
        &self,
        _config: &ExperimentConfig<ExperimentRunBase>,
        _globals: &Globals,
        _field_spec_map_builder: &mut FieldSpecMapBuilder,
    ) -> Result<()> {
        Ok(())
    }

    fn add_state_field_specs(
        &self,
        _config: &ExperimentConfig<ExperimentRunBase>,
        _globals: &Globals,
        _field_spec_map_builder: &mut FieldSpecMapBuilder,
    ) -> Result<()> {
        Ok(())
    }
}

pub struct ContextColumn {
    inner: Box<dyn ContextColumnWriter + Send + Sync>,
}

impl ContextColumn {
    pub fn get_dynamic_metadata(&self) -> DatastoreResult<ColumnDynamicMetadata> {
        self.inner.get_dynamic_metadata()
    }

    pub fn write(&self, buffer: &mut [u8], meta: &ColumnDynamicMetadata) -> DatastoreResult<()> {
        self.inner.write(buffer, meta)
    }
}

pub trait ContextColumnWriter {
    fn get_dynamic_metadata(&self) -> DatastoreResult<ColumnDynamicMetadata>;
    fn write(&self, buffer: &mut [u8], meta: &ColumnDynamicMetadata) -> DatastoreResult<()>;
}