#[cfg(test)]
mod test;

use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::Receiver;
use futures::future::{self, BoxFuture};
use futures::never::Never;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::PendingClasses;
use papyrus_network::network_manager::network_manager_metrics::{
    NetworkManagerMetrics,
    SqmrNetworkMetrics,
};
use papyrus_network::network_manager::{self, NetworkError, NetworkManager};
use papyrus_p2p_sync::client::{
    P2pSyncClient,
    P2pSyncClientChannels,
    P2pSyncClientConfig,
    P2pSyncClientError,
};
use papyrus_p2p_sync::server::{P2pSyncServer, P2pSyncServerChannels};
use papyrus_p2p_sync::{Protocol, BUFFER_SIZE};
use papyrus_storage::{open_storage, StorageReader, StorageWriter};
use papyrus_sync::sources::central::{CentralError, CentralSource};
use papyrus_sync::sources::pending::PendingSource;
use papyrus_sync::{
    StateSync as CentralStateSync,
    StateSyncError as CentralStateSyncError,
    GENESIS_HASH,
};
use starknet_api::block::BlockHash;
use starknet_api::felt;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::component_server::WrapperServer;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_sequencer_metrics::metric_definitions::{
    STATE_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
    STATE_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
    STATE_SYNC_NUM_CONNECTED_PEERS,
};
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tokio::sync::RwLock;

use crate::config::{CentralSyncClientConfig, StateSyncConfig};

pub struct StateSyncRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    // TODO(Matan): change client and server to requester and responder respectively
    p2p_sync_client_future: BoxFuture<'static, Result<Never, P2pSyncClientError>>,
    p2p_sync_server_future: BoxFuture<'static, Never>,
    central_sync_client_future: BoxFuture<'static, Result<(), CentralStateSyncError>>,
    new_block_dev_null_future: BoxFuture<'static, Never>,
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
                unreachable!("Return type Never should never be constructed")
            }
            result = &mut self.central_sync_client_future => {
                result.map_err(|_| ComponentError::InternalComponentError)
            }
            _never = &mut self.new_block_dev_null_future => {
                unreachable!("Return type Never should never be constructed")
            }

        }
    }
}

impl StateSyncRunner {
    pub fn new(
        config: StateSyncConfig,
        new_block_receiver: Receiver<SyncBlock>,
        class_manager_client: SharedClassManagerClient,
    ) -> (Self, StorageReader) {
        let StateSyncConfig {
            storage_config,
            p2p_sync_client_config,
            central_sync_client_config,
            network_config,
        } = config;

        let network_manager_metrics = Some(NetworkManagerMetrics {
            num_connected_peers: STATE_SYNC_NUM_CONNECTED_PEERS,
            num_active_inbound_sessions: STATE_SYNC_NUM_ACTIVE_INBOUND_SESSIONS,
            num_active_outbound_sessions: STATE_SYNC_NUM_ACTIVE_OUTBOUND_SESSIONS,
            broadcast_metrics: None,
            sqmr_metrics: Some(SqmrNetworkMetrics {}),
        });
        let mut network_manager = network_manager::NetworkManager::new(
            network_config,
            Some(VERSION_FULL.to_string()),
            network_manager_metrics,
        );

        let (storage_reader, storage_writer) =
            open_storage(storage_config).expect("StateSyncRunner failed opening storage");

        // Creating the sync server future
        let p2p_sync_server = Self::new_p2p_state_sync_server(
            storage_reader.clone(),
            &mut network_manager,
            class_manager_client.clone(),
        );
        let p2p_sync_server_future = p2p_sync_server.run().boxed();

        // Creating the sync clients futures
        // Exactly one of the sync clients must be turned on.
        let (p2p_sync_client_future, central_sync_client_future, new_block_dev_null_future) =
            match (p2p_sync_client_config, central_sync_client_config) {
                (Some(p2p_sync_client_config), None) => {
                    let p2p_sync_client = Self::new_p2p_state_sync_client(
                        storage_reader.clone(),
                        storage_writer,
                        p2p_sync_client_config,
                        &mut network_manager,
                        new_block_receiver,
                        class_manager_client,
                    );
                    let p2p_sync_client_future = p2p_sync_client.run().boxed();
                    let central_sync_client_future = future::pending().boxed();
                    let new_block_dev_null_future = future::pending().boxed();
                    (p2p_sync_client_future, central_sync_client_future, new_block_dev_null_future)
                }
                (None, Some(central_sync_client_config)) => {
                    let central_sync_client = Self::new_central_state_sync_client(
                        storage_reader.clone(),
                        storage_writer,
                        central_sync_client_config,
                        class_manager_client,
                    );
                    let p2p_sync_client_future = future::pending().boxed();
                    let central_sync_client_future = central_sync_client.run().boxed();
                    let new_block_dev_null_future =
                        create_new_block_receiver_future_dev_null(new_block_receiver);

                    (p2p_sync_client_future, central_sync_client_future, new_block_dev_null_future)
                }
                _ => {
                    panic!(
                        "It is validated that exactly one of --sync.#is_none or \
                         --p2p_sync.#is_none must be turned on"
                    )
                }
            };
        (
            Self {
                network_future: network_manager.run().boxed(),
                p2p_sync_client_future,
                p2p_sync_server_future,
                central_sync_client_future,
                new_block_dev_null_future,
            },
            storage_reader,
        )
    }

    fn new_p2p_state_sync_client(
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        p2p_sync_client_config: P2pSyncClientConfig,
        network_manager: &mut NetworkManager,
        new_block_receiver: Receiver<SyncBlock>,
        class_manager_client: SharedClassManagerClient,
    ) -> P2pSyncClient {
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
        P2pSyncClient::new(
            p2p_sync_client_config,
            storage_reader,
            storage_writer,
            p2p_sync_client_channels,
            new_block_receiver.boxed(),
            class_manager_client.clone(),
        )
    }

    fn new_p2p_state_sync_server(
        storage_reader: StorageReader,
        network_manager: &mut NetworkManager,
        class_manager_client: SharedClassManagerClient,
    ) -> P2pSyncServer {
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
        P2pSyncServer::new(storage_reader, p2p_sync_server_channels, class_manager_client)
    }

    fn new_central_state_sync_client(
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        central_sync_client_config: CentralSyncClientConfig,
        class_manager_client: SharedClassManagerClient,
    ) -> CentralStateSync {
        let CentralSyncClientConfig { sync_config, central_source_config } =
            central_sync_client_config;
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
        let base_layer_source = None;
        CentralStateSync::new(
            sync_config,
            shared_highest_block,
            pending_data,
            pending_classes,
            central_source,
            pending_source,
            base_layer_source,
            storage_reader,
            storage_writer,
            Some(class_manager_client),
        )
    }
}

/// A future that consumes the new block receiver and does nothing with the received blocks, to
/// prevent the buffer from filling up.
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
