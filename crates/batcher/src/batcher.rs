use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_storage::test_utils::get_test_storage;
use starknet_mempool_infra::component_runner::ComponentStarter;
use starknet_mempool_types::communication::SharedMempoolClient;

use crate::config::BatcherConfig;

// TODO(Tsabary/Yael/Dafna): Replace with actual batcher code.
pub struct Batcher {
    pub config: BatcherConfig,
    pub mempool_client: SharedMempoolClient,
    pub storage: Arc<dyn BatcherStorageReaderTrait>,
}

impl Batcher {
    pub fn new(
        config: BatcherConfig,
        mempool_client: SharedMempoolClient,
        storage: Arc<dyn BatcherStorageReaderTrait>,
    ) -> Self {
        Self { config, mempool_client, storage }
    }
}

pub fn create_batcher(config: BatcherConfig, mempool_client: SharedMempoolClient) -> Batcher {
    // let (storage_reader, _storage_writer) = papyrus_storage::open_storage(config.storage.clone())
    //     .expect("Failed to open batcher's storage");
    let (storage_reader, _storage_writer) = get_test_storage().0;
    Batcher::new(config, mempool_client, Arc::new(storage_reader))
}

#[async_trait]
impl ComponentStarter for Batcher {}

#[cfg_attr(test, automock)]
pub trait BatcherStorageReaderTrait: Send + Sync {}

impl BatcherStorageReaderTrait for papyrus_storage::StorageReader {}
