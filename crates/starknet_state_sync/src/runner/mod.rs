#[cfg(test)]
mod test;

use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::FutureExt;
use papyrus_common::pending_classes::PendingClasses;
use papyrus_storage::{open_storage, StorageReader};
use papyrus_sync::sources::base_layer::EthereumBaseLayerSource;
use papyrus_sync::sources::central::CentralSource;
use papyrus_sync::sources::pending::PendingSource;
use papyrus_sync::{
    StateSync as PapyrusStateSync,
    StateSyncError as PapyrusStateSyncError,
    GENESIS_HASH,
};
use starknet_api::block::BlockHash;
use starknet_api::felt;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use tokio::sync::RwLock;

use crate::config::StateSyncConfig;

pub struct StateSyncRunner {
    #[allow(dead_code)]
    request_receiver: mpsc::Receiver<(StateSyncRequest, oneshot::Sender<StateSyncResponse>)>,
    #[allow(dead_code)]
    storage_reader: StorageReader,
    sync_future: BoxFuture<'static, Result<(), PapyrusStateSyncError>>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        // TODO(shahak): poll request_receiver.
        tokio::select! {
            result = &mut self.sync_future => result.map_err(|_| ComponentError::InternalComponentError)
        }
    }
}

impl StateSyncRunner {
    pub fn new(
        config: StateSyncConfig,
        request_receiver: mpsc::Receiver<(StateSyncRequest, oneshot::Sender<StateSyncResponse>)>,
    ) -> Self {
        let (storage_reader, storage_writer) =
            open_storage(config.storage_config).expect("StateSyncRunner failed opening storage");

        let shared_highest_block = Arc::new(RwLock::new(None));
        let pending_data = Arc::new(RwLock::new(PendingData {
            // The pending data might change later to DeprecatedPendingBlock, depending on the
            // response from the feeder gateway.
            block: PendingBlockOrDeprecated::Current(PendingBlock {
                parent_block_hash: BlockHash(felt!(GENESIS_HASH)),
                ..Default::default()
            }),
            ..Default::default()
        }));
        let pending_classes = Arc::new(RwLock::new(PendingClasses::default()));

        let central_source =
            CentralSource::new(config.central_config.clone(), VERSION_FULL, storage_reader.clone())
                .expect("Failed creating CentralSource");
        // TODO(shahak): add the ability to disable pending sync and disable it here.
        let pending_source = PendingSource::new(config.central_config, VERSION_FULL)
            .expect("Failed creating PendingSource");
        let base_layer_source = EthereumBaseLayerSource::new(config.base_layer_config)
            .expect("Failed creating base layer");
        let sync = PapyrusStateSync::new(
            config.sync_config,
            shared_highest_block,
            pending_data,
            pending_classes,
            central_source,
            pending_source,
            base_layer_source,
            storage_reader.clone(),
            storage_writer,
        );
        let sync_future = sync.run().boxed();

        // TODO(shahak): add rpc.
        Self { request_receiver, storage_reader, sync_future }
    }
}

// TODO(shahak): fill with a proper version, or allow not specifying the node version.
const VERSION_FULL: &str = "";
