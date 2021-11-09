use crate::simulation::task::result::TaskResultOrCancelled;
use crate::{proto, worker};
use thiserror::Error as ThisError;
use tokio::sync::mpsc::error::SendError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("{0}")]
    Unique(String),

    #[error("Worker error: {0}")]
    Worker(#[from] worker::Error),

    #[error("Simulation error: {0}")]
    Simulation(#[from] crate::simulation::Error),

    #[error("Datastore error: {0}")]
    Datastore(#[from] crate::datastore::Error),

    #[error("Tokio Join Error: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),

    #[error("Task result send error: {0}")]
    TaskResultSend(SendError<TaskResultOrCancelled>),

    #[error("Missing worker with index {0}")]
    MissingWorkerWithIndex(crate::types::WorkerIndex),

    #[error("Kill message already sent")]
    KillMessageAlreadySent,

    #[error("Kill message not sent")]
    KillMessageNotSent,

    #[error("Kill confirm already sent")]
    KillConfirmAlreadySent,

    #[error("Missing simulation with id {0}")]
    MissingSimulationWithID(proto::SimulationShortID),

    #[error("Channel for sending cancel task messages has unexpectedly closed")]
    CancelClosed,

    #[error("Missing worker controllers")]
    MissingWorkerControllers,

    #[error("The communications with worker controllers has been unexpectedly dropped")]
    UnexpectedWorkerCommsDrop,

    #[error("Missing pending task with id {0}")]
    MissingPendingTask(crate::types::TaskID),

    #[error("Missing one-shot task result sender to send result with")]
    NoResultSender,
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Unique(s.to_string())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Unique(s)
    }
}

impl<T> From<SendError<T>> for Error
where
    T: std::fmt::Debug,
{
    fn from(e: SendError<T>) -> Self {
        Error::Unique(format!("Tokio Send Error: {:?}", e))
    }
}