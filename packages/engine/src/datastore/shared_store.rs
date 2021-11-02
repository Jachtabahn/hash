use super::Result;
use crate::proto::ExperimentRunBase;
use std::{collections::HashMap, sync::Arc};

use crate::config::ExperimentConfig;
use crate::datastore::batch::Dataset;

pub struct SharedStore {
    pub datasets: HashMap<String, Arc<Dataset>>,
}

impl SharedStore {
    pub fn new(config: &ExperimentConfig<ExperimentRunBase>) -> Result<SharedStore> {
        let datasets = &config.run.project_base.datasets;
        let mut dataset_batches = HashMap::with_capacity(datasets.len());
        for dataset in &config.run.project_base.datasets {
            let dataset_name = dataset.shortname.clone();
            let dataset_batch = Dataset::new_from_dataset(dataset, &config.run_id.to_string())?;
            dataset_batches.insert(dataset_name, Arc::new(dataset_batch));
        }

        Ok(SharedStore {
            datasets: dataset_batches,
        })
    }
}