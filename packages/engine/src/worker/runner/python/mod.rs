mod error;

use std::ops::Deref;
use crate::gen;
use futures::FutureExt;
use nng::options::Options;
use nng::{Aio, Socket};
use std::str::FromStr;
use arrow::datatypes::Schema;
use arrow::ipc::writer::schema_to_bytes;
use flatbuffers::{FlatBufferBuilder, ForwardsUOffset, Vector, WIPOffset};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, Receiver, Sender, UnboundedReceiver, UnboundedSender};
use uuid::Uuid;

use super::comms::{
    inbound::InboundToRunnerMsgPayload, outbound::OutboundFromRunnerMsg, ExperimentInitRunnerMsg,
};
use crate::datastore::batch::Batch;
use crate::proto::{ExperimentID, SimulationShortID};
use crate::types::WorkerIndex;
use crate::worker::{Error as WorkerError, Result as WorkerResult};
pub use error::{Error, Result};
use crate::datastore::arrow::util::arrow_continuation;
use crate::datastore::prelude::SharedStore;
use crate::datastore::table::sync::StateSync;
use crate::datastore::table::task_shared_store::{PartialSharedState, SharedContext, SharedState};
use crate::gen::{Metaversion, StateInterimSyncArgs};
use crate::simulation::enum_dispatch::TaskSharedStore;
use crate::worker::runner::comms::inbound::InboundToRunnerMsgPayload::StateInterimSync;
use crate::worker::runner::comms::PackageMsgs;

fn pkgs_to_fbs<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    pkgs: &PackageMsgs,
) -> Result<WIPOffset<crate::gen::PackageConfig<'f>>> {
    let pkgs = pkgs.0
        .iter()
        .map(|(package_id, init_msg)| {
            let package_name = fbb.create_string(init_msg.name.clone().into());

            let serialized_payload = fbb.create_vector(
                &serde_json::to_vec(&init_msg.payload)?
            );
            let payload = gen::Serialized::create(
                fbb,
                &gen::SerializedArgs {
                    inner: Some(serialized_payload),
                },
            );

            Ok(gen::Package::create(
                fbb,
                &gen::PackageArgs {
                    type_: init_msg.r#type.into(),
                    name: Some(package_name),
                    sid: package_id.as_usize() as u64,
                    init_payload: Some(payload),
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let pkgs = fbb.create_vector(&pkgs);
    Ok(gen::PackageConfig::create(
        fbb,
        &gen::PackageConfigArgs {
            packages: Some(pkgs),
        },
    ))
}

fn shared_ctx_to_fbs<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    shared_ctx: &SharedStore
) -> WIPOffset<crate::gen::SharedContext<'f>> {
    let mut batch_offsets = Vec::new();
    for (_, dataset) in shared_ctx.datasets.iter() {
        batch_offsets.push(batch_to_fbs(fbb, dataset));
    }
    // let batch_offsets: Vec<_> = shared_ctx.datasets
    //     .iter()
    //     .map(|(_name, dataset)| batch_to_fbs(fbb, dataset))
    //     .collect();
    let batch_fbs_vec = fbb.create_vector(&batch_offsets);

    // Build the SharedContext using the vec
    gen::SharedContext::create(
        fbb,
        &gen::SharedContextArgs {
            datasets: Some(batch_fbs_vec),
        },
    )
}

fn experiment_init_to_nng(init: &ExperimentInitRunnerMsg) -> Result<nng::Message> {
    // TODO - initial buffer size
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let experiment_id = gen::ExperimentID(*(Uuid::from_str(&init.experiment_id)?.as_bytes()));

    // Build the SharedContext Flatbuffer Batch objects and collect their offsets in a vec
    let shared_context = shared_ctx_to_fbs(&mut fbb, &init.shared_context);

    // Build the Flatbuffer Package objects and collect their offsets in a vec
    let package_config = pkgs_to_fbs(&mut fbb, &init.package_config)?;
    let msg = gen::Init::create(
        &mut fbb,
        &crate::gen::InitArgs {
            experiment_id: Some(&experiment_id),
            worker_index: init.worker_index as u64,
            shared_context: Some(shared_context),
            package_config: Some(package_config),
        },
    );

    fbb.finish(msg, None);
    let bytes = fbb.finished_data();

    let mut nanomsg = nng::Message::with_capacity(bytes.len());
    nanomsg.push_front(bytes);

    Ok(nanomsg)
}

fn metaversion_to_fbs<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    metaversion: &crate::datastore::batch::metaversion::Metaversion
) -> WIPOffset<crate::gen::Metaversion<'f>> {
    gen::Metaversion::create(
        fbb,
        &gen::MetaversionArgs {
            memory: metaversion.memory(),
            batch: metaversion.batch(),
        },
    )
}

fn batch_to_fbs<'f, B: Batch, T: Deref<Target=B>>(
    fbb: &mut FlatBufferBuilder<'f>,
    batch: &T
) -> WIPOffset<crate::gen::Batch<'f>> {
    let batch_id_offset = fbb.create_string(batch.get_batch_id());
    let metaversion_offset = metaversion_to_fbs(fbb, batch.metaversion());
    gen::Batch::create(
        fbb,
        &gen::BatchArgs {
            batch_id: Some(batch_id_offset),
            metaversion: Some(metaversion_offset),
        },
    )
}

fn shared_store_to_fbs<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    shared_store: TaskSharedStore
) -> WIPOffset<crate::gen::StateInterimSync<'f>> {
    let (agent_mv, msg_mv, indices) = match shared_store.state {
        SharedState::None => (vec![], vec![], vec![]),
        SharedState::Read(state) => {
            let a: Vec<_> = state.agent_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
            let m: Vec<_> = state.message_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
            let indices = (0..a.len()).collect();
            (a, m, indices)
        }
        SharedState::Write(state) => {
            let a: Vec<_> = state.agent_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
            let m: Vec<_> = state.message_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
            let indices = (0..a.len()).collect();
            (a, m, indices)
        }
        SharedState::Partial(partial) => match partial {
            PartialSharedState::Read(partial) => {
                let state = partial.inner;
                let a: Vec<_> = state.agent_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
                let m: Vec<_> = state.message_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
                (a, m, partial.indices)
            }
            PartialSharedState::Write(partial) => {
                let state = partial.inner;
                let a: Vec<_> = state.agent_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
                let m: Vec<_> = state.message_pool().batches().iter().map(
                |b| metaversion_to_fbs(fbb,b.metaversion())
            ).collect();
                (a, m, partial.indices)
            }
        }
    };
    let indices: Vec<_> = indices.into_iter().map(|i| i as u32).collect();
    let args = StateInterimSyncArgs {
        group_idx: Some(fbb.create_vector(&indices)),
        agent_pool_metaversions: Some(fbb.create_vector(&agent_mv)),
        message_pool_metaversions: Some(fbb.create_vector(&msg_mv)),
    };
    crate::gen::StateInterimSync::create(
        fbb,
        &args
    )
}

fn str_to_serialized<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    s: &str
) -> WIPOffset<crate::gen::Serialized<'f>> {
    let inner = fbb.create_vector(s.as_bytes());
    crate::gen::Serialized::create(
        fbb,
        &crate::gen::SerializedArgs {
            inner: Some(inner)
        }
    )
}

fn state_sync_to_fbs<'f>(
    fbb: &mut FlatBufferBuilder<'f>,
    msg: StateSync,
) -> Result<(
    WIPOffset<Vector<'f, ForwardsUOffset<crate::gen::Batch<'f>>>>,
    WIPOffset<Vector<'f, ForwardsUOffset<crate::gen::Batch<'f>>>>
)> {
    let agent_pool = msg.agent_pool.read_batches()?;
    let agent_pool: Vec<_> = agent_pool
        .iter()
        .map(|batch| batch_to_fbs(fbb, batch))
        .collect();
    let agent_pool = fbb.create_vector(&agent_pool);

    let msg_pool = msg.message_pool.read_batches()?;
    let msg_pool: Vec<_> = msg_pool
        .iter()
        .map(|batch| batch_to_fbs(fbb, batch))
        .collect();
    let msg_pool = fbb.create_vector(&msg_pool);

    Ok((agent_pool, msg_pool))
}

// TODO: Code duplication with JS runner; move this function into datastore?
fn schema_to_stream_bytes(schema: &Schema) -> Vec<u8> {
    let content = schema_to_bytes(schema);
    let mut stream_bytes = arrow_continuation(content.len());
    stream_bytes.extend_from_slice(&content);
    stream_bytes
}

fn inbound_to_nng(
    sim_id: Option<SimulationShortID>,
    msg: InboundToRunnerMsgPayload,
) -> Result<nng::Message> {
    let mut fbb = flatbuffers::FlatBufferBuilder::new();
    let fbb = &mut fbb;

    let (msg, msg_type) = match msg {
        InboundToRunnerMsgPayload::TaskMsg(msg) => {
            let shared_store = shared_store_to_fbs(fbb, msg.shared_store);

            let payload = serde_json::to_string(&msg.payload).unwrap();
            let payload = str_to_serialized(fbb,&payload);

            let task_id = crate::gen::runner_inbound_msg_generated::TaskID(
                msg.task_id.to_le_bytes()
            );

            let msg = crate::gen::runner_inbound_msg_generated::TaskMsg::create(
                fbb,
                &crate::gen::runner_inbound_msg_generated::TaskMsgArgs {
                    package_sid: msg.package_id.as_usize() as u64,
                    task_id: Some(&task_id),
                    payload: Some(payload),
                    metaversioning: Some(shared_store),
                }
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::TaskMsg)
        }
        InboundToRunnerMsgPayload::CancelTask(_) => todo!(), // Unused for now
        InboundToRunnerMsgPayload::StateSync(msg) => {
            let (agent_pool, message_pool) = state_sync_to_fbs(fbb, msg)?;
            let msg = crate::gen::StateSync::create(
                fbb,
                &crate::gen::StateSyncArgs {
                    agent_pool: Some(agent_pool),
                    message_pool: Some(message_pool),
                    current_step: -1 // TODO: current_step shouldn't be propagated here
                }
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::StateSync)
        }
        InboundToRunnerMsgPayload::StateSnapshotSync(msg) => {
            let (agent_pool, message_pool) = state_sync_to_fbs(fbb, msg)?;
            let msg = crate::gen::StateSnapshotSync::create(
                fbb,
                &crate::gen::StateSnapshotSyncArgs {
                    agent_pool: Some(agent_pool),
                    message_pool: Some(message_pool),
                    current_step: -1 // TODO: current_step shouldn't be propagated here
                }
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::StateSnapshotSync)
        }
        InboundToRunnerMsgPayload::ContextBatchSync(msg) => {
            let batch = msg.context_batch.read();
            let batch = batch_to_fbs(fbb, &batch);
            let msg = crate::gen::ContextBatchSync::create(
                fbb,
                &crate::gen::ContextBatchSyncArgs {
                    context_batch: Some(batch),
                    current_step: -1 // TODO: Should have current_step in ContextBatchSync
                }
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::ContextBatchSync)
        }
        InboundToRunnerMsgPayload::StateInterimSync(msg) => {
            let msg = shared_store_to_fbs(fbb, msg.shared_store);
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::StateInterimSync)
        }
        InboundToRunnerMsgPayload::TerminateSimulationRun => {
            let msg = crate::gen::runner_inbound_msg_generated::TerminateSimulationRun::create(
                fbb,
                &crate::gen::TerminateSimulationRunArgs {}
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::TerminateSimulationRun)
        }
        InboundToRunnerMsgPayload::KillRunner => {
            let msg = crate::gen::runner_inbound_msg_generated::KillRunner::create(
                fbb,
                &crate::gen::KillRunnerArgs {}
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::KillRunner)
        }
        InboundToRunnerMsgPayload::NewSimulationRun(msg) => {
            let _sim_id = fbb.create_string(""); // TODO: Remove `sim_id` from fbs.

            let globals = serde_json::to_string(&msg.globals.0)
                .expect("Can serialize serde_json::Value");
            let globals = fbb.create_string(&globals);

            let package_config = pkgs_to_fbs(fbb, &msg.packages)?;

            let shared_ctx = shared_ctx_to_fbs(fbb, &msg.datastore.shared_store);
            let mut agent_schema_bytes = schema_to_stream_bytes(
                &msg.datastore.agent_batch_schema.arrow
            );
            let mut msg_schema_bytes = schema_to_stream_bytes(
                &msg.datastore.message_batch_schema
            );
            let mut ctx_schema_bytes = schema_to_stream_bytes(
                &msg.datastore.context_batch_schema
            );
            let agent_schema_bytes = fbb.create_vector(&agent_schema_bytes);
            let msg_schema_bytes = fbb.create_vector(&msg_schema_bytes);
            let ctx_schema_bytes = fbb.create_vector(&ctx_schema_bytes);
            let datastore_init = crate::gen::DatastoreInit::create(
                fbb,
                &crate::gen::DatastoreInitArgs {
                    agent_batch_schema: Some(agent_schema_bytes),
                    message_batch_schema: Some(msg_schema_bytes),
                    context_batch_schema: Some(ctx_schema_bytes),
                    shared_context: Some(shared_ctx)
                }
            );

            let msg = crate::gen::NewSimulationRun::create(
                fbb,
                &crate::gen::NewSimulationRunArgs {
                    sim_id: Some(_sim_id),
                    sid: msg.short_id,
                    properties: Some(globals),
                    package_config: Some(package_config),
                    datastore_init: Some(datastore_init),
                }
            );
            (msg.as_union_value(), crate::gen::RunnerInboundMsgPayload::NewSimulationRun)
        }
    };

    let msg = crate::gen::RunnerInboundMsg::create(
        fbb,
        &crate::gen::RunnerInboundMsgArgs {
            sim_sid: sim_id.unwrap_or(0),
            payload_type: msg_type,
            payload: Some(msg)
        }
    );
    fbb.finish(msg, None);
    let bytes = fbb.finished_data();

    let mut nanomsg = nng::Message::with_capacity(bytes.len());
    nanomsg.push_front(bytes);
    Ok(nanomsg)
}

/// Only used for sending messages to the Python process
struct NngSender {
    route: String,

    // Used in the aio to send nng messages to the Python process.
    to_py: Socket,
    aio: Aio,

    // Sends the results of operations (i.e. results of trying to
    // send nng messages) in the aio.
    // aio_result_sender: UnboundedSender<Result<()>>,

    // Receives the results of operations from the aio.
    aio_result_receiver: UnboundedReceiver<Result<()>>,
}

impl NngSender {
    fn new(experiment_id: ExperimentID, worker_index: WorkerIndex) -> Result<Self> {
        let route = format!("ipc://{}-topy{}", experiment_id, worker_index);
        let to_py = Socket::new(nng::Protocol::Pair0)?;
        to_py.set_opt::<nng::options::SendBufferSize>(30)?;
        // TODO: Stress test to determine whether send buffer size is sufficiently large

        let (aio_result_sender, aio_result_receiver) = unbounded_channel();
        let aio = Aio::new(move |_aio, res| match res {
            nng::AioResult::Send(res) => {
                match res {
                    Ok(_) => {
                        aio_result_sender.send(Ok(())).unwrap();
                    }
                    Err((msg, err)) => {
                        log::warn!("External worker receiving socket tried to send but failed w/ error: {}", err);
                        match aio_result_sender.send(Err(Error::NngSend(msg, err))) {
                            Ok(_) => {}
                            Err(err) => {
                                log::warn!(
                                    "Failed to pass send error back to message handler thread {}",
                                    err
                                );
                            }
                        };
                    }
                }
            }
            nng::AioResult::Sleep(res) => match res {
                Err(err) => {
                    log::error!("AIO sleep error: {}", err);
                    aio_result_sender.send(Err(Error::Nng(err))).unwrap();
                }
                _ => {}
            },
            nng::AioResult::Recv(_) => {
                unreachable!("This callback is only for the send operation")
            }
        })?;
        aio.set_timeout(Some(std::time::Duration::new(5, 0)))?;

        Ok(Self {
            route,
            to_py,
            aio,
            aio_result_receiver,
        })
    }

    fn send(
        &self,
        sim_id: Option<SimulationShortID>,
        msg: InboundToRunnerMsgPayload,
    ) -> Result<()> {
        // TODO: (option<SimId>, inbound payload) --> flatbuffers --> nng
        let msg = inbound_to_nng(sim_id, msg)?;
        self.aio.wait();
        self.to_py
            .send_async(&self.aio, msg)
            .map_err(|(msg, err)| {
                log::warn!("Send failed: {:?}", (&msg, &err));
                Error::NngSend(msg, err)
            })?;
        Ok(())
    }

    async fn get_send_result(&mut self) -> Option<Result<()>> {
        self.aio_result_receiver.recv().await
    }
}

/// Only used for receiving messages from the Python process,
/// except for the init message, which is sent once in response
/// to an init message request
struct NngReceiver {
    route: String,

    // Used in the aio to receive nng messages from the Python process.
    from_py: Socket,
    aio: Aio,

    // Sends the results of operations (i.e. results of trying to
    // receive nng messages) in the aio.
    // aio_result_sender: UnboundedSender<nng::Message>,

    // Receives the results of operations from the aio.
    aio_result_receiver: UnboundedReceiver<nng::Message>,
}

impl NngReceiver {
    pub fn new(experiment_id: ExperimentID, worker_index: WorkerIndex) -> Result<Self> {
        let route = format!("ipc://{}-frompy{}", experiment_id, worker_index);
        let from_py = Socket::new(nng::Protocol::Pair0)?;

        let (aio_result_sender, aio_result_receiver) = unbounded_channel();
        let aio = Aio::new(move |_aio, res| match res {
            nng::AioResult::Recv(Ok(m)) => {
                aio_result_sender.send(m).expect("Should be able to send");
            }
            nng::AioResult::Sleep(Ok(_)) => {}
            nng::AioResult::Send(_) => {
                log::warn!("Unexpected send result");
            }
            nng::AioResult::Recv(Err(nng::Error::Canceled)) => {}
            nng::AioResult::Recv(Err(nng::Error::Closed)) => {}
            _ => panic!("Error in the AIO, {:?}", res),
        })?;

        Ok(Self {
            route,
            from_py,
            aio,
            aio_result_receiver,
        })
    }

    pub fn init(&self, init_msg: &ExperimentInitRunnerMsg) -> Result<()> {
        self.from_py.listen(&self.route)?;

        let listener = nng::Listener::new(&self.from_py, &self.route)?;
        let _init_request = self.from_py.recv()?;
        self.from_py // Only case where `from_py` is used for sending
            .send(experiment_init_to_nng(init_msg)?)
            .map_err(|(msg, err)| Error::NngSend(msg, err))?;

        let _init_ack = self.from_py.recv()?;
        listener.close();
        Ok(())
    }

    async fn get_recv_result(&mut self) -> Result<OutboundFromRunnerMsg> {
        let nng_msg = self
            .aio_result_receiver
            .recv()
            .await
            .ok_or(Error::OutboundReceive)?;

        self.from_py.recv_async(&self.aio)?;
        Ok(OutboundFromRunnerMsg::from(nng_msg))
    }
}

pub struct PythonRunner {
    init_msg: ExperimentInitRunnerMsg,
    nng_sender: NngSender,
    nng_receiver: NngReceiver,
    kill_sender: Sender<()>,
    kill_receiver: Receiver<()>,
    spawned: bool,
}

impl PythonRunner {
    pub fn new(spawn: bool, init: ExperimentInitRunnerMsg) -> WorkerResult<Self> {
        let nng_sender = NngSender::new(init.experiment_id.clone(), init.worker_index)?;
        let nng_receiver = NngReceiver::new(init.experiment_id.clone(), init.worker_index)?;
        let (kill_sender, kill_receiver) = tokio::sync::mpsc::channel(2);
        Ok(Self {
            init_msg: init,
            spawned: spawn,
            nng_sender,
            nng_receiver,
            kill_sender,
            kill_receiver,
        })
    }

    pub async fn send(
        &self,
        sim_id: Option<SimulationShortID>,
        msg: InboundToRunnerMsgPayload,
    ) -> WorkerResult<()> {
        self.nng_sender.send(sim_id, msg)?;
        // if matches!(msg, InboundToRunnerMsgPayload::KillRunner) {
        //     self.kill_sender.send(()).await.map_err(|e| Error::KillSend(e))?;
        // }
        Ok(())
    }

    // TODO: Duplication with other runners (move into worker?)
    pub async fn send_if_spawned(
        &self,
        sim_id: Option<SimulationShortID>,
        msg: InboundToRunnerMsgPayload,
    ) -> WorkerResult<()> {
        if self.spawned {
            self.send(sim_id, msg).await?;
        }
        Ok(())
    }

    pub async fn recv(&mut self) -> WorkerResult<OutboundFromRunnerMsg> {
        self.nng_receiver
            .get_recv_result()
            .await
            .map_err(WorkerError::from)
    }

    // TODO: Duplication with other runners (move into worker?)
    pub async fn recv_now(&mut self) -> WorkerResult<Option<OutboundFromRunnerMsg>> {
        self.recv().now_or_never().transpose()
    }

    // TODO: Duplication with other runners (move into worker?)
    pub fn spawned(&self) -> bool {
        self.spawned
    }

    pub async fn run(&mut self) -> WorkerResult<()> {
        // TODO: Duplication with other runners (move into worker?)
        if !self.spawned {
            return Ok(());
        }

        // Spawn Python process.
        let mut cmd = Command::new("sh");
        cmd.arg(".src/worker/runner/python/run.sh")
            .arg(&self.init_msg.experiment_id)
            .arg(&self.init_msg.worker_index.to_string());
        let mut process = cmd.spawn().map_err(|e| Error::Spawn(e))?;

        // Send messages to Python process.
        self.nng_receiver.init(&self.init_msg)?;
        loop {
            tokio::select! {
                Some(nng_send_result) = self.nng_sender.get_send_result() => {
                    nng_send_result?;
                }
                Some(_) = self.kill_receiver.recv() => {
                    break;
                }
            }
        }

        // // TODO: Drop nng_sender/nng_receiver before killing process?
        // match await_timeout(process.wait(), std::time::Duration::from_secs(10))? {
        //     None => {
        //         log::info!("Python process has failed to exit; killing.");
        //         process.kill().await?;
        //     }
        //     Some(status) => {
        //         log::info!(
        //             "Python runner has successfully exited with status: {:?}.",
        //             status.code().unwrap_or(-1)
        //         );
        //     }
        // }
        Ok(())
    }
}