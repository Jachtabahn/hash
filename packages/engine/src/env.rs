use crate::proto::{EngineMsg, EngineStatus, ExecutionEnvironment, ExperimentRunRepr, InitMessage};
use crate::{nano, Args};
use serde::Deserialize;
use thiserror::Error as ThisError;
use tokio::time::Duration;

pub type Result<T, E = Error> = std::result::Result<T, E>;

lazy_static! {
    static ref INIT_MSG_RECV_TIMEOUT: Duration = Duration::from_secs(60);
}

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("Env error: {0}")]
    Unique(String),

    #[error("Nano error: {0}")]
    Nano(#[from] nano::Error),

    #[error("Unexpected message to the engine, expected an init message")]
    UnexpectedEngineMsgExpectedInit,
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

pub struct OrchClient {
    url: String,
    experiment_id: String,
    client: nano::Client,
}

impl OrchClient {
    pub fn new(url: &str, experiment_id: &str) -> Result<Self> {
        let client = nano::Client::new(url, 1)?;
        Ok(OrchClient {
            url: url.into(),
            experiment_id: experiment_id.into(),
            client,
        })
    }

    pub async fn send(&mut self, msg: EngineStatus) -> Result<()> {
        let m = crate::proto::OrchestratorMsg {
            experiment_id: self.experiment_id.clone(),
            body: msg,
        };
        tokio::time::timeout(Duration::from_secs(5), self.client.send(&m))
            .await
            .map_err(|_| Error::from("send engine status timeout"))?
            .map_err(Error::from)
    }

    pub fn try_clone(&self) -> Result<Self> {
        OrchClient::new(&self.url, &self.experiment_id)
    }
}

pub struct Environment<E: ExperimentRunRepr> {
    pub orch_client: OrchClient,
    pub orch_listener: nano::Server,
    pub experiment: E, // todo extended experiment run??
    pub execution_env: ExecutionEnvironment,
    pub dyn_payloads: serde_json::Map<String, serde_json::Value>,
}

pub async fn env<E>(args: &Args) -> Result<Environment<E>>
where
    E: ExperimentRunRepr + for<'de> Deserialize<'de>,
{
    log::info!("Persist data to S3: {}", args.persist); // TODO - Doesn't look like it does anything
    let mut orch_client = OrchClient::new(&args.orchestrator_url, &args.experiment_id)?;
    log::debug!("Connected to orchestrator at {}", &args.orchestrator_url);

    let mut orch_listener = nano::Server::new(&args.listen_url)?;
    log::debug!("Listening on NNG socket at {}", &args.listen_url);

    // Before it will send the init message, we must tell the orchestrator that the
    // engine has started
    orch_client.send(EngineStatus::Started).await?;
    log::debug!("Sent started message");

    // Wait for the init message from the orchestrator
    let InitMessage {
        experiment,
        env: execution_env,
        dyn_payloads,
    } = recv_init_msg(&mut orch_listener).await?;
    log::debug!("Received initialization message from the orchestrator");

    return Ok(Environment {
        orch_client,
        orch_listener,
        experiment,
        execution_env,
        dyn_payloads,
    });
}

async fn recv_init_msg<E: ExperimentRunRepr>(
    orch_listener: &mut nano::Server,
) -> Result<InitMessage<E>> {
    let msg = tokio::time::timeout(*INIT_MSG_RECV_TIMEOUT, orch_listener.recv::<EngineMsg<E>>())
        .await
        .map_err(|_| Error::from("receive init message timeout"))??;

    return match msg {
        EngineMsg::Init(init) => Ok(init),
        _ => Err(Error::UnexpectedEngineMsgExpectedInit),
    };
}