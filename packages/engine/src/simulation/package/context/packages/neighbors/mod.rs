use std::sync::Arc;

use crate::config::{Globals, TopologyConfig};
use crate::datastore::batch::AgentBatch;
use crate::datastore::schema::accessor::FieldSpecMapAccessor;
use crate::datastore::schema::context::ContextSchema;
use crate::datastore::schema::FieldSpecMapBuilder;
use crate::datastore::table::state::view::StateSnapshot;
use crate::datastore::table::state::State;
use crate::datastore::{
    batch::iterators,
    table::{pool::agent::AgentPool, state::ReadState},
};
use crate::proto::ExperimentRunBase;
use crate::simulation::comms::package::PackageComms;
use crate::simulation::package::context::{ContextColumn, Package, PackageCreator};
use crate::simulation::package::ext_traits::{
    GetWorkerExpStartMsg, GetWorkerSimStartMsg, MaybeCPUBound,
};
use crate::simulation::package::prelude::{ArrowArray, ContextPackage};
use crate::simulation::Result;
use crate::{ExperimentConfig, SimRunConfig};
use async_trait::async_trait;
use parking_lot::RwLockReadGuard;
use serde_json::Value;

use self::map::{NeighborMap, NeighborRef};

mod adjacency;
mod fields;
mod map;
mod writer;

const CPU_BOUND: bool = true;
pub const NEIGHBOR_INDEX_COUNT: usize = 2;
pub type IndexType = u32;
pub type ArrowIndexBuilder = arrow::array::UInt32Builder;

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
    ) -> Result<Box<dyn ContextPackage>> {
        let neighbors = Neighbors {
            topology: Arc::new(
                TopologyConfig::create_from_globals(&config.sim.globals)
                    .unwrap_or_else(|_| TopologyConfig::default()),
            ),
        };
        Ok(Box::new(neighbors))
    }

    fn add_context_field_specs(
        &self,
        _config: &ExperimentConfig<ExperimentRunBase>,
        _globals: &Globals,
        field_spec_map_builder: &mut FieldSpecMapBuilder,
    ) -> Result<()> {
        fields::add_context(field_spec_map_builder)?;
        Ok(())
    }

    fn add_state_field_specs(
        &self,
        _config: &ExperimentConfig<ExperimentRunBase>,
        _globals: &Globals,
        field_spec_map_builder: &mut FieldSpecMapBuilder,
    ) -> Result<()> {
        fields::add_state(field_spec_map_builder)?;
        Ok(())
    }
}

impl GetWorkerExpStartMsg for Creator {
    fn get_worker_exp_start_msg(&self) -> Result<Value> {
        Ok(Value::Null)
    }
}

struct Neighbors {
    topology: Arc<TopologyConfig>,
}

impl Neighbors {
    fn neighbor_vec<'a>(
        batches: &'a Vec<RwLockReadGuard<AgentBatch>>,
    ) -> Result<Vec<NeighborRef<'a>>> {
        Ok(iterators::agent::position_iter(batches)?
            .zip(iterators::agent::index_iter(batches))
            .zip(iterators::agent::search_radius_iter(batches)?)
            .collect())
    }
}

impl MaybeCPUBound for Neighbors {
    fn cpu_bound(&self) -> bool {
        CPU_BOUND
    }
}

impl GetWorkerSimStartMsg for Neighbors {
    fn get_worker_sim_start_msg(&self) -> Result<Value> {
        Ok(Value::Null)
    }
}

#[async_trait]
impl Package for Neighbors {
    async fn run<'s>(
        &mut self,
        state: Arc<State>,
        snapshot: Arc<StateSnapshot>,
    ) -> Result<ContextColumn> {
        let agent_pool = state.agent_pool();
        let batches = agent_pool.read_batches()?;
        let states = Self::neighbor_vec(&batches)?;
        let map = NeighborMap::gather(states, &self.topology)?;

        Ok(ContextColumn {
            inner: Box::new(map),
        })
    }

    fn get_empty_arrow_column(
        &self,
        num_agents: usize,
        _schema: &ContextSchema,
    ) -> Result<Arc<dyn arrow::array::Array>> {
        let index_builder = ArrowIndexBuilder::new(1024);

        let neighbor_index_builder = arrow::array::FixedSizeListBuilder::new(index_builder, 2);
        let mut neighbors_builder = arrow::array::ListBuilder::new(neighbor_index_builder);

        (0..num_agents).try_for_each(|_| neighbors_builder.append(true))?;

        Ok(Arc::new(neighbors_builder.finish()) as Arc<dyn ArrowArray>)
    }
}