use super::super::*;
use crate::proto::InitialStateName;
use crate::simulation::{Error, Result};
use serde_json::Value;

pub struct Creator {}

impl Creator {
    pub fn new() -> Box<dyn PackageCreator> {
        Box::new(Creator {})
    }
}

impl PackageCreator for Creator {
    fn create(
        &self,
        config: &Arc<SimRunConfig<ExperimentRunBase>>,
        _comms: PackageComms,
        accessor: FieldSpecMapAccessor,
    ) -> Result<Box<dyn InitPackage>> {
        match &config.exp.run.project_base.initial_state.name {
            InitialStateName::InitJson | InitialStateName::InitJs => Ok(Box::new(Package {
                initial_state_src: config.exp.run.project_base.initial_state.src.clone(),
            })
                as Box<dyn InitPackage>),
            name => {
                return Err(Error::from(format!("Trying to create a JSON init package but the init file didn't end in .json: {:?}", name)));
            }
        }
    }
}

impl GetWorkerExpStartMsg for Creator {
    fn get_worker_exp_start_msg(&self) -> Result<Value> {
        // TODO possibly pass init.json here to optimize
        Ok(Value::Null)
    }
}
pub struct Package {
    initial_state_src: String,
}

impl MaybeCPUBound for Package {
    fn cpu_bound(&self) -> bool {
        false
    }
}

impl GetWorkerSimStartMsg for Package {
    fn get_worker_sim_start_msg(&self) -> Result<Value> {
        Ok(Value::Null)
    }
}

#[async_trait]
impl InitPackage for Package {
    async fn run(&mut self) -> Result<Vec<Agent>> {
        // TODO Map Error when we design package errors
        serde_json::from_str(&self.initial_state_src).map_err(|e| {
            Error::from(format!(
                "Failed to parse agent state JSON to Vec<Agent>: {:?}",
                e
            ))
        })
    }
}