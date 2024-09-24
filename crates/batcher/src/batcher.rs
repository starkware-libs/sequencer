use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::config::BatcherConfig;

// TODO(Tsabary/Yael/Dafna): Replace with actual batcher code.
pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    pub storage: Arc<dyn BatcherStorageTrait>,
}

impl Batcher {
    pub fn new(
        config: BatcherConfig,
        mempool_client: SharedMempoolClient,
        local_storage: Arc<dyn BatcherStorageTrait>,
    ) -> Self {
        Self { config, mempool_client, storage: local_storage }
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
        .expect("Failed to open batcher's storage");
    Batcher::new(config, mempool_client, Arc::new(storage_reader))
}

#[async_trait]
impl ComponentStarter for Batcher {}

#[cfg_attr(test, automock)]
pub trait BatcherStorageTrait: Send + Sync {}

impl BatcherStorageTrait for papyrus_storage::StorageReader {}
