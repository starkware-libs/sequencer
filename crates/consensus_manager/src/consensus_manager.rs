use async_trait::async_trait;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::info;

use crate::config::ConsensusManagerConfig;

// TODO(Tsabary/Matan): Replace with actual consensus manager code.

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
}

impl ConsensusManager {
    pub fn new(config: ConsensusManagerConfig, batcher_client: SharedBatcherClient) -> Self {
        Self { config, batcher_client }
    }
}

pub fn create_consensus_manager(
    config: ConsensusManagerConfig,
    batcher_client: SharedBatcherClient,
) -> ConsensusManager {
    ConsensusManager::new(config, batcher_client)
}

#[async_trait]
impl ComponentStarter for ConsensusManager {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("ConsensusManager::start()");
        Ok(())
    }
}
