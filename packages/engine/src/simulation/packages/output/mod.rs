pub mod packages;

use std::sync::Arc;

pub use crate::config::Globals;
use crate::datastore::schema::FieldSpecMapBuilder;
use crate::proto::ExperimentRunBase;
use crate::simulation::comms::package::PackageComms;
use crate::SimRunConfig;
pub use packages::{Name, OutputTask, OutputTaskMessage, OutputTaskResult, PACKAGES};

use self::packages::Output;

use super::prelude::*;
use super::{
    deps::Dependencies,
    ext_traits::{GetWorkerStartMsg, MaybeCPUBound},
};

pub trait PackageCreator: Sync {
    /// Create the package.
    fn create(
        &self,
        config: &Arc<SimRunConfig<ExperimentRunBase>>,
        system: PackageComms,
    ) -> Result<Box<dyn Package>>;

    fn get_dependencies(&self) -> Result<Dependencies> {
        Ok(Dependencies::empty())
    }

    fn persistence_config(
        &self,
        config: &ExperimentConfig<ExperimentRunBase>,
        globals: &Globals,
    ) -> Result<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn add_state_field_specs(
        &self,
        config: &ExperimentConfig<ExperimentRunBase>,
        globals: &Globals,
        field_spec_map_builder: &mut FieldSpecMapBuilder,
    ) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
pub trait Package: MaybeCPUBound + GetWorkerStartMsg + Send + Sync {
    async fn run<'s>(&mut self, state: Arc<State>, context: Arc<Context>) -> Result<Output>;
}