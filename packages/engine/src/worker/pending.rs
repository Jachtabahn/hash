use std::collections::HashMap;

use crate::{types::TaskID, Language};

use super::task::WorkerTask;

pub enum CancelState {
    Active(Vec<Language>),
    None,
}

impl Default for CancelState {
    fn default() -> Self {
        CancelState::None
    }
}

#[derive(new)]
pub struct PendingWorkerTask {
    pub inner: WorkerTask,
    pub active_runner: Language,
    #[new(default)]
    pub cancelling: CancelState,
}

#[derive(Default)]
pub struct PendingWorkerTasks {
    pub inner: HashMap<TaskID, PendingWorkerTask>,
}