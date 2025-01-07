#[cfg(test)]
mod test;

use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::Receiver;
use futures::future::{self, BoxFuture};
use futures::never::Never;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::PendingClasses;
use papyrus_network::network_manager::{self, NetworkError};
use papyrus_network::NetworkConfig;
use papyrus_p2p_sync::client::{
    P2pSyncClient,
    P2pSyncClientChannels,
    P2pSyncClientConfig,
    P2pSyncClientError,
};
use papyrus_p2p_sync::server::{P2pSyncServer, P2pSyncServerChannels};
use papyrus_p2p_sync::{Protocol, BUFFER_SIZE};
use papyrus_storage::{open_storage, StorageConfig, StorageReader};
use papyrus_sync::sources::base_layer::EthereumBaseLayerSource;
use papyrus_sync::sources::central::{CentralError, CentralSource};
use papyrus_sync::sources::pending::PendingSource;
use papyrus_sync::{StateSync, StateSyncError, GENESIS_HASH};
use starknet_api::block::BlockHash;
use starknet_api::felt;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::component_server::WrapperServer;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tokio::sync::RwLock;

use crate::config::{CentralSyncClientConfig, StateSyncConfig};

pub struct StateSyncRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    // TODO: change client and server to requester and responder respectively
    p2p_sync_client_future: BoxFuture<'static, Result<Never, P2pSyncClientError>>,
    p2p_sync_server_future: BoxFuture<'static, Never>,
    // TODO(alonl): remove this annotation
    central_sync_client_future: BoxFuture<'static, Result<(), StateSyncError>>, /* Is this the right error? */
    #[allow(dead_code)]
    new_block_receiver_future: BoxFuture<'static, Never>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) -> Result<(), ComponentError> {
        tokio::select! {
            result = &mut self.network_future => {
                result.map_err(|_| ComponentError::InternalComponentError)
            }
            result = &mut self.p2p_sync_client_future => {
                result.map_err(|_| ComponentError::InternalComponentError).map(|_never| ())
            }
            _never = &mut self.p2p_sync_server_future => {
                Err(ComponentError::InternalComponentError)
            }
            result = &mut self.central_sync_client_future => {
                result.map_err(|_| ComponentError::InternalComponentError)
            }
            _never = &mut self.new_block_receiver_future => {
                Err(ComponentError::InternalComponentError)
            }

        }
    }
}

impl StateSyncRunner {
    pub fn new(
        config: StateSyncConfig,
        new_block_receiver: Receiver<SyncBlock>,
    ) -> (Self, StorageReader) {
        let StateSyncConfig {
            storage_config,
            p2p_sync_client_config,
            central_sync_client_config,
            network_config,
        } = config;
        match (p2p_sync_client_config, central_sync_client_config) {
            (Some(p2p_sync_client_config), None) => Self::new_p2p_state_sync(
                p2p_sync_client_config,
                storage_config,
                network_config,
                new_block_receiver,
            ),
            (None, Some(central_sync_client_config)) => Self::new_central_state_sync(
                central_sync_client_config,
                storage_config,
                new_block_receiver,
            ),
            (None, None) => (
                Self {
                    network_future: future::pending().boxed(),
                    p2p_sync_client_future: future::pending().boxed(),
                    p2p_sync_server_future: future::pending().boxed(),
                    central_sync_client_future: future::pending().boxed(),
                    new_block_receiver_future: create_new_block_receiver_future_dev_null(
                        new_block_receiver,
                    ),
                },
                open_storage(storage_config).expect("StateSyncRunner failed opening storage").0,
            ),
            (Some(_), Some(_)) => {
                unreachable!(
                    "It is validated that one of --sync.#is_none or --p2p_sync.#is_none must be \
                     turned on"
                )
            }
        }
    }

    fn new_p2p_state_sync(
        p2p_sync_client_config: P2pSyncClientConfig,
        storage_config: StorageConfig,
        network_config: NetworkConfig,
        new_block_receiver: Receiver<SyncBlock>,
    ) -> (Self, StorageReader) {
        let (storage_reader, storage_writer) =
            open_storage(storage_config).expect("StateSyncRunner failed opening storage");

        let mut network_manager =
            network_manager::NetworkManager::new(network_config, Some(VERSION_FULL.to_string()));

        let header_client_sender = network_manager
            .register_sqmr_protocol_client(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
        let state_diff_client_sender =
            network_manager.register_sqmr_protocol_client(Protocol::StateDiff.into(), BUFFER_SIZE);
        let transaction_client_sender = network_manager
            .register_sqmr_protocol_client(Protocol::Transaction.into(), BUFFER_SIZE);
        let class_client_sender =
            network_manager.register_sqmr_protocol_client(Protocol::Class.into(), BUFFER_SIZE);
        let p2p_sync_client_channels = P2pSyncClientChannels::new(
            header_client_sender,
            state_diff_client_sender,
            transaction_client_sender,
            class_client_sender,
        );
        let p2p_sync_client = P2pSyncClient::new(
            p2p_sync_client_config,
            storage_reader.clone(),
            storage_writer,
            p2p_sync_client_channels,
            new_block_receiver.boxed(),
        );

        let header_server_receiver = network_manager
            .register_sqmr_protocol_server(Protocol::SignedBlockHeader.into(), BUFFER_SIZE);
        let state_diff_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::StateDiff.into(), BUFFER_SIZE);
        let transaction_server_receiver = network_manager
            .register_sqmr_protocol_server(Protocol::Transaction.into(), BUFFER_SIZE);
        let class_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::Class.into(), BUFFER_SIZE);
        let event_server_receiver =
            network_manager.register_sqmr_protocol_server(Protocol::Event.into(), BUFFER_SIZE);
        let p2p_sync_server_channels = P2pSyncServerChannels::new(
            header_server_receiver,
            state_diff_server_receiver,
            transaction_server_receiver,
            class_server_receiver,
            event_server_receiver,
        );
        let p2p_sync_server = P2pSyncServer::new(storage_reader.clone(), p2p_sync_server_channels);

        let network_future = network_manager.run().boxed();
        let p2p_sync_client_future = p2p_sync_client.run().boxed();
        let p2p_sync_server_future = p2p_sync_server.run().boxed();

        // TODO(shahak): add rpc.
        (
            Self {
                network_future,
                p2p_sync_client_future,
                p2p_sync_server_future,
                central_sync_client_future: future::pending().boxed(),
                new_block_receiver_future: future::pending().boxed(),
            },
            storage_reader,
        )
    }

    fn new_central_state_sync(
        central_sync_client_config: CentralSyncClientConfig,
        storage_config: StorageConfig,
        new_block_receiver: Receiver<SyncBlock>,
    ) -> (Self, StorageReader) {
        let CentralSyncClientConfig { sync_config, central_source_config, base_layer_config } =
            central_sync_client_config;
        let (storage_reader, storage_writer) =
            open_storage(storage_config).expect("StateSyncRunner failed opening storage");
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
            CentralSource::new(central_source_config.clone(), VERSION_FULL, storage_reader.clone())
                .map_err(CentralError::ClientCreation)
                .expect("CentralSource creation failed in central sync");
        let pending_source = PendingSource::new(central_source_config, VERSION_FULL)
            .map_err(CentralError::ClientCreation)
            .expect("PendingSource creation failed in central sync");
        let base_layer_source = EthereumBaseLayerSource::new(base_layer_config);
        let sync = StateSync::new(
            sync_config,
            shared_highest_block,
            pending_data,
            pending_classes,
            central_source,
            pending_source,
            base_layer_source,
            storage_reader.clone(),
            storage_writer,
        );
        let central_sync_client_future = sync.run().boxed();
        (
            Self {
                network_future: future::pending().boxed(),
                p2p_sync_client_future: future::pending().boxed(),
                p2p_sync_server_future: future::pending().boxed(),
                central_sync_client_future,
                new_block_receiver_future: create_new_block_receiver_future_dev_null(
                    new_block_receiver,
                ),
            },
            storage_reader,
        )
    }
}

fn create_new_block_receiver_future_dev_null(
    mut new_block_receiver: Receiver<SyncBlock>,
) -> BoxFuture<'static, Never> {
    async move {
        loop {
            let _sync_block = new_block_receiver.next().await;
        }
    }
    .boxed()
}

pub type StateSyncRunnerServer = WrapperServer<StateSyncRunner>;
// TODO(shahak): fill with a proper version, or allow not specifying the node version.
const VERSION_FULL: &str = "";
