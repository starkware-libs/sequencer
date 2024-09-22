use async_trait::async_trait;
use starknet_mempool_infra::component_definitions::ComponentMonitor;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::config::BatcherConfig;

// TODO(Tsabary/Yael/Dafna): Replace with actual batcher code.
pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
}

impl Batcher {
    pub fn new(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Self {
        Self { config, mempool_client }
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    Batcher::new(config, mempool_client)
}

#[async_trait]
impl ComponentStarter for Batcher {}

#[async_trait]
impl ComponentMonitor for Batcher {}
