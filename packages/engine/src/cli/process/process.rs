use super::error::Result;
use async_trait::async_trait;
use hash_prime::proto::{EngineMsg, ExperimentRunRepr};

#[async_trait]
pub trait Process {
    async fn exit_and_cleanup(self: Box<Self>) -> Result<()>;
    async fn send<E: ExperimentRunRepr>(&mut self, msg: &EngineMsg<E>) -> Result<()>;
}

#[async_trait]
pub trait Command {
    async fn run(self: Box<Self>) -> Result<Box<dyn Process + Send>>;
}