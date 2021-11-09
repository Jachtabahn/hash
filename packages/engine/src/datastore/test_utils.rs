use std::convert::TryInto;
use std::sync::Arc;

use crate::hash_types::state::{Agent, AgentStateField};
use rand::{prelude::StdRng, Rng, SeedableRng};

use serde::{Deserialize, Serialize};

use crate::datastore::error::Error;
use crate::datastore::schema::state::AgentSchema;
use crate::datastore::schema::{
    FieldScope, FieldSource, FieldSpec, FieldSpecMap, FieldType, FieldTypeVariant, RootFieldSpec,
};

lazy_static! {
    pub static ref JSON_KEYS: serde_json::Value = serde_json::json!({
        "defined": {
            "foo": {
                "bar": "boolean",
                "baz": "[number]",
                "qux": "[number; 4]",
                "quux": "[string; 16]?",
            }
        },
        "keys": {
            "complex": {
                "position": "[number; 2]",
                "abc": "[[foo; 6]]"
            },
            "fixed_of_variable" : "[[number]; 2]",
            "seed": "number"
        }
    });
}

#[derive(Serialize, Deserialize)]
pub struct Foo {
    bar: bool,
    baz: Vec<f64>,
    qux: [f64; 4],
    quux: Option<[String; 16]>,
}

fn rand_string(seed: u64) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let count = rng.gen_range(0..64);
    String::from_utf8(
        std::iter::repeat(())
            .map(|()| rng.sample(rand::distributions::Alphanumeric))
            .take(count)
            .collect::<Vec<u8>>(),
    )
    .unwrap()
}

impl Foo {
    fn new(seed: u64) -> Foo {
        let mut rng = StdRng::seed_from_u64(seed);
        Foo {
            bar: rng.gen(),
            baz: (0..rng.gen_range(0..8)).map(|_| rng.gen()).collect(),
            qux: rng.gen(),
            quux: {
                if rng.gen_bool(0.7) {
                    Some(arr_macro::arr![rand_string(seed); 16])
                } else {
                    None
                }
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Complex {
    position: [f64; 2],
    abc: Vec<[Foo; 6]>,
}

impl Complex {
    fn new(seed: u64) -> Complex {
        let mut rng = StdRng::seed_from_u64(seed);

        Complex {
            position: rng.gen(),
            abc: (0..rng.gen_range(0..8))
                .map(|_| arr_macro::arr![Foo::new(seed); 6])
                .collect(),
        }
    }
}

fn make_dummy_agent(seed: u64) -> Result<Agent, Error> {
    let mut rng = StdRng::seed_from_u64(seed);

    let mut agent = Agent::empty();
    let id = uuid::Uuid::new_v4().to_hyphenated().to_string();
    agent.set(AgentStateField::AgentId.name(), &id)?;
    // We do an implicit conversion to f64 for number types so for testing need to ensure
    // manually created agents only have f64
    agent.set("age", Some(rng.gen::<f64>()))?;
    agent.set(AgentStateField::AgentName.name(), rand_string(seed))?;

    let rand_vec1 = (0..rng.gen_range(0..14))
        .map(|_| rng.gen_range(-10_f64..13.5))
        .collect::<Vec<_>>();
    let rand_vec2 = (0..rng.gen_range(0..23))
        .map(|_| rng.gen_range(-2.6_f64..12.5))
        .collect::<Vec<_>>();
    agent.set("fixed_of_variable", vec![rand_vec1, rand_vec2])?;
    agent.set("complex", Complex::new(seed))?;
    // see above per f64
    agent.set("seed", Some(seed as f64))?;

    Ok(agent)
}

/// Creates a vec of dummy Agent states for testing and accompanying Agent schema
pub fn gen_schema_and_test_agents(
    num_agents: usize,
    seed: u64,
) -> Result<(Arc<AgentSchema>, Vec<Agent>), Error> {
    let mut fields = FieldSpecMap::default();
    fields.add(RootFieldSpec {
        inner: FieldSpec {
            name: "age".into(),
            field_type: FieldType {
                variant: FieldTypeVariant::Number,
                nullable: false,
            },
        },
        scope: FieldScope::Agent,
        source: FieldSource::Engine,
    });

    fields.add(AgentStateField::AgentId.try_into()?)?;
    fields.add(AgentStateField::AgentName.try_into()?)?;

    fields.union(FieldSpecMap::from_short_json(
        JSON_KEYS.clone(),
        FieldSource::Engine,
        FieldScope::Agent,
    )?)?;

    let schema = Arc::new(AgentSchema::new(fields)?);

    let mut agents = Vec::with_capacity(num_agents);
    for i in 0..num_agents {
        let agent_seed = (seed as usize + i) as u64;
        agents.push(make_dummy_agent(agent_seed)?);
    }

    Ok((schema, agents))
}