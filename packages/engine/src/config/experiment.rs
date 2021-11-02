use std::sync::Arc;

use crate::config::globals::Globals;
use crate::proto::{ExperimentRunBase, ExperimentRunRepr};

use super::{package, worker, worker_pool, Result};

use crate::proto::ExperimentID;

#[derive(Clone)]
/// Experiment level configuration
pub struct Config<E: ExperimentRunRepr> {
    pub run_id: Arc<ExperimentID>, // we need this only for non-pod runs TODO remove and create random internal ids?
    pub packages: Arc<package::Config>,
    pub run: Arc<E>,
    pub worker_pool: Arc<worker_pool::Config>,
    pub base_globals: Globals,
}

impl<E: ExperimentRunRepr> Config<E> {
    pub(super) fn new(experiment_run: E, max_num_workers: usize) -> Result<Config<E>> {
        // For differentiation purposes when multiple experiment runs are active in the same system
        let run_id = uuid::Uuid::new_v4().to_string();
        let packages = Arc::new(package::ConfigBuilder::new().build()?);
        let base_globals = Globals::from_json(serde_json::from_str(
            &experiment_run.base().project_base.globals_src,
        )?)?;

        let run = Arc::new(experiment_run);

        let worker_base_config = worker::Config::default(); // TODO ask packages for what language execution they require
                                                            // this would mean that Rust will not be in them
        let worker_pool = Arc::new(worker_pool::Config::new(
            worker_base_config,
            max_num_workers,
        ));

        Ok(Config {
            run_id: Arc::new(run_id),
            packages,
            run,
            base_globals,
            worker_pool,
        })
    }

    // Downcast this config
    pub fn to_base(&self) -> Result<Config<ExperimentRunBase>> {
        let run_base = self.run.base().clone();
        Ok(Config {
            run_id: self.run_id.clone(),
            packages: self.packages.clone(),
            run: Arc::new(run_base),
            worker_pool: self.worker_pool.clone(),
            base_globals: self.base_globals.clone(),
        })
    }
}