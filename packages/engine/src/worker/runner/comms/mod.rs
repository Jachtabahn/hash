use std::{collections::HashMap, fmt, sync::Arc};

use parking_lot::RwLock;

use crate::config::Globals;
use crate::datastore::schema::state::AgentSchema;
use crate::datastore::shared_store::SharedStore;
use crate::datastore::table::pool::BatchPool;
use crate::proto::{ExperimentID, SimulationShortID};
use crate::simulation::packages::id::PackageId;
use crate::{
    datastore::prelude::{AgentBatch, ArrowSchema, Dataset, MessageBatch},
    simulation::{packages::worker_init::PackageInitMsgForWorker, task::prelude::TaskMessage},
    types::{TaskID, WorkerIndex},
    Language,
};

pub mod inbound;
pub mod outbound;

#[derive(Debug)]
pub enum MessageTarget {
    Rust,
    Python,
    JavaScript,
    Dynamic,
    Main,
}

impl From<Language> for MessageTarget {
    fn from(l: Language) -> Self {
        match l {
            Language::Rust => Self::Rust,
            Language::Python => Self::Python,
            Language::JavaScript => Self::JavaScript,
        }
    }
}

#[derive(Debug)]
pub struct RunnerTaskMsg {
    pub package_id: PackageId,
    pub task_id: TaskID,
    pub sync: StateInterimSync,
    pub payload: TaskMessage,
    pub group_index: Option<u32>,
}

#[derive(Debug)]
pub struct TargetedRunnerTaskMsg {
    pub target: MessageTarget,
    pub msg: RunnerTaskMsg,
}

pub struct StateInterimSync {
    pub group_indices: Vec<usize>,
    pub agent_batches: Box<dyn BatchPool<AgentBatch>>,
    pub message_batches: Box<dyn BatchPool<MessageBatch>>,
    // shared state
}

impl fmt::Debug for StateInterimSync {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        f.write_str(&format!(
            "StateInterimSync ( group_indices: {:?}, ... )",
            self.group_indices,
        ))
    }
}

pub struct DatastoreInit {
    pub agent_batch_schema: Vec<u8>,
    pub message_batch_schema: Vec<u8>,
    pub context_batch_schema: Vec<u8>,
    pub shared_context: SharedStore,
}

#[derive(Clone)]
pub struct PackageMsgs(pub HashMap<PackageId, PackageInitMsgForWorker>);

pub struct NewSimulationRun {
    pub short_id: SimulationShortID,
    pub packages: PackageMsgs,
    pub datastore: DatastoreSimulationPayload,
    pub globals: Arc<Globals>,
}

#[derive(new)]
pub struct DatastoreSimulationPayload {
    pub agent_batch_schema: Arc<AgentSchema>,
    pub message_batch_schema: Arc<ArrowSchema>,
    pub context_batch_schema: Arc<ArrowSchema>,
    pub shared_store: Arc<SharedStore>,
}

#[derive(Clone)]
pub struct ExperimentInitRunnerMsg {
    pub experiment_id: ExperimentID,
    pub worker_index: WorkerIndex,
    pub shared_context: SharedStore,
    pub package_config: PackageMsgs,
}