#[cfg(test)]
mod test;

use std::sync::Arc;

use apollo_central_sync::metrics::{
    SYNC_BODY_MARKER,
    SYNC_CLASS_MANAGER_MARKER,
    SYNC_COMPILED_CLASS_MARKER,
    SYNC_HEADER_MARKER,
    SYNC_PROCESSED_TRANSACTIONS,
    SYNC_REVERTED_TRANSACTIONS,
    SYNC_STATE_MARKER,
};
use apollo_central_sync::sources::central::{CentralError, CentralSource};
use apollo_central_sync::sources::pending::PendingSource;
use apollo_central_sync::{
    StateSync as CentralStateSync,
    StateSyncError as CentralStateSyncError,
    GENESIS_HASH,
};
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_network::network_manager::metrics::{NetworkMetrics, SqmrNetworkMetrics};
use apollo_network::network_manager::{self, NetworkError, NetworkManager};
use apollo_p2p_sync::client::{
    P2pSyncClient,
    P2pSyncClientChannels,
    P2pSyncClientConfig,
    P2pSyncClientError,
};
use apollo_p2p_sync::server::{P2pSyncServer, P2pSyncServerChannels};
use apollo_p2p_sync::{Protocol, BUFFER_SIZE};
use apollo_reverts::{revert_block, revert_blocks_and_eternal_pending};
use apollo_rpc::{run_server, RpcConfig};
use apollo_sequencer_infra::component_definitions::ComponentStarter;
use apollo_sequencer_infra::component_server::WrapperServer;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_storage::compiled_class::CasmStorageReader;
use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::{open_storage, StorageConfig, StorageReader, StorageTxn, StorageWriter};
use async_trait::async_trait;
use futures::channel::mpsc::Receiver;
use futures::future::{self, pending, BoxFuture};
use futures::never::Never;
use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::PendingClasses;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::felt;
use starknet_client::reader::objects::pending_data::{PendingBlock, PendingBlockOrDeprecated};
use starknet_client::reader::PendingData;
use tokio::sync::RwLock;

use crate::config::{CentralSyncClientConfig, StateSyncConfig};
use crate::metrics::{
    STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
    STATE_SYNC_P2P_NUM_BLACKLISTED_PEERS,
    STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
};

pub struct StateSyncRunner {
    network_future: BoxFuture<'static, Result<(), NetworkError>>,
    // TODO(Matan): change client and server to requester and responder respectively
    p2p_sync_client_future: BoxFuture<'static, Result<Never, P2pSyncClientError>>,
    p2p_sync_server_future: BoxFuture<'static, Never>,
    central_sync_client_future: BoxFuture<'static, Result<(), CentralStateSyncError>>,
    new_block_dev_null_future: BoxFuture<'static, Never>,
    rpc_server_future: BoxFuture<'static, ()>,
}

#[async_trait]
impl ComponentStarter for StateSyncRunner {
    async fn start(&mut self) {
        tokio::select! {
            _ = &mut self.network_future => {
                panic!("StateSyncRunner failed - network stopped unexpectedly");
            }
            _ = &mut self.p2p_sync_client_future => {
                panic!("StateSyncRunner failed - p2p sync client stopped unexpectedly");
            }
            _never = &mut self.p2p_sync_server_future => {
                unreachable!("Return type Never should never be constructed")
            }
            _ = &mut self.central_sync_client_future => {
                panic!("StateSyncRunner failed - central sync client stopped unexpectedly");
            }
            _never = &mut self.new_block_dev_null_future => {
                unreachable!("Return type Never should never be constructed")
            }
            _ = &mut self.rpc_server_future => {
                unreachable!("JSON_RPC server stopped unexpectedly");
            }
        }
    }
}

pub struct StateSyncResources {
    pub storage_reader: StorageReader,
    pub storage_writer: StorageWriter,
    pub shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pub pending_data: Arc<RwLock<PendingData>>,
    pub pending_classes: Arc<RwLock<PendingClasses>>,
}

impl StateSyncResources {
    pub fn new(storage_config: &StorageConfig) -> Self {
        let (storage_reader, storage_writer) =
            open_storage(storage_config.clone()).expect("StateSyncRunner failed opening storage");
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
        Self { storage_reader, storage_writer, shared_highest_block, pending_data, pending_classes }
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
            revert_config,
            rpc_config,
        } = config;

        let StateSyncResources {
            storage_reader,
            mut storage_writer,
            shared_highest_block,
            pending_data,
            pending_classes,
        } = StateSyncResources::new(&storage_config);

        register_metrics(&storage_reader.begin_ro_txn().unwrap());

        if revert_config.should_revert {
            let revert_up_to_and_including = revert_config.revert_up_to_and_including;
            // We assume that sync always writes the headers before any other block data.
            let current_header_marker = storage_reader
                .begin_ro_txn()
                .expect("Should be able to begin read only transaction")
                .get_header_marker()
                .expect("Should have a header marker");

            let revert_block_fn = move |current_block_number| {
                let n_reverted_txs = storage_writer
                    .begin_rw_txn()
                    .unwrap()
                    .get_block_transactions_count(current_block_number)
                    .unwrap()
                    .unwrap_or(0)
                    .try_into()
                    .expect("Failed to convert usize to u64");
                revert_block(&mut storage_writer, current_block_number);
                update_marker_metrics(&storage_writer.begin_rw_txn().unwrap());
                SYNC_REVERTED_TRANSACTIONS.increment(n_reverted_txs);
                async {}
            };

            return (
                Self {
                    network_future: pending().boxed(),
                    p2p_sync_client_future: revert_blocks_and_eternal_pending(
                        current_header_marker,
                        revert_up_to_and_including,
                        revert_block_fn,
                        "State Sync",
                    )
                    .map(|_never| unreachable!("Never should never be constructed"))
                    .boxed(),
                    p2p_sync_server_future: pending().boxed(),
                    central_sync_client_future: pending().boxed(),
                    new_block_dev_null_future: pending().boxed(),
                    rpc_server_future: pending().boxed(),
                },
                storage_reader,
            );
        }

        let network_manager_metrics = Some(NetworkMetrics {
            num_connected_peers: STATE_SYNC_P2P_NUM_CONNECTED_PEERS,
            num_blacklisted_peers: STATE_SYNC_P2P_NUM_BLACKLISTED_PEERS,
            broadcast_metrics_by_topic: None,
            sqmr_metrics: Some(SqmrNetworkMetrics {
                num_active_inbound_sessions: STATE_SYNC_P2P_NUM_ACTIVE_INBOUND_SESSIONS,
                num_active_outbound_sessions: STATE_SYNC_P2P_NUM_ACTIVE_OUTBOUND_SESSIONS,
            }),
        });
        let mut network_manager = network_manager::NetworkManager::new(
            network_config,
            Some(VERSION_FULL.to_string()),
            network_manager_metrics,
        );

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
                        class_manager_client.clone(),
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
                        shared_highest_block.clone(),
                        pending_data.clone(),
                        pending_classes.clone(),
                        central_sync_client_config,
                        class_manager_client.clone(),
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

        let rpc_server_future = spawn_rpc_server(
            &rpc_config,
            shared_highest_block.clone(),
            pending_data.clone(),
            pending_classes.clone(),
            storage_reader.clone(),
            Some(class_manager_client.clone()),
        );

        (
            Self {
                network_future: network_manager.run().boxed(),
                p2p_sync_client_future,
                p2p_sync_server_future,
                central_sync_client_future,
                new_block_dev_null_future,
                rpc_server_future,
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
        shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
        pending_data: Arc<RwLock<PendingData>>,
        pending_classes: Arc<RwLock<PendingClasses>>,
        central_sync_client_config: CentralSyncClientConfig,
        class_manager_client: SharedClassManagerClient,
    ) -> CentralStateSync {
        let CentralSyncClientConfig { sync_config, central_source_config } =
            central_sync_client_config;
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
            storage_reader.clone(),
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

// Create JSON-RPC server
fn spawn_rpc_server(
    rpc_config: &RpcConfig,
    shared_highest_block: Arc<RwLock<Option<BlockHashAndNumber>>>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    storage_reader: StorageReader,
    class_manager_client: Option<SharedClassManagerClient>,
) -> BoxFuture<'static, ()> {
    let rpc_config = rpc_config.clone();
    async move {
        let (_, server_handle) = run_server(
            &rpc_config,
            shared_highest_block,
            pending_data,
            pending_classes,
            storage_reader,
            VERSION_FULL,
            class_manager_client,
        )
        .await
        .expect("Failed running JSON-RPC server");
        tokio::spawn(async move {
            server_handle.stopped().await;
        })
        .await
        .expect("Failed spawning JSON-RPC server");
    }
    .boxed()
}

fn register_metrics<Mode: TransactionKind>(txn: &StorageTxn<'_, Mode>) {
    SYNC_HEADER_MARKER.register();
    SYNC_BODY_MARKER.register();
    SYNC_STATE_MARKER.register();
    SYNC_CLASS_MANAGER_MARKER.register();
    SYNC_COMPILED_CLASS_MARKER.register();
    SYNC_PROCESSED_TRANSACTIONS.register();
    SYNC_REVERTED_TRANSACTIONS.register();
    update_marker_metrics(txn);
    reconstruct_processed_transactions_metric(txn);
}

fn update_marker_metrics<Mode: TransactionKind>(txn: &StorageTxn<'_, Mode>) {
    SYNC_HEADER_MARKER.set_lossy(txn.get_header_marker().expect("Should have a header marker").0);
    SYNC_BODY_MARKER.set_lossy(txn.get_body_marker().expect("Should have a body marker").0);
    SYNC_STATE_MARKER.set_lossy(txn.get_state_marker().expect("Should have a state marker").0);
    SYNC_CLASS_MANAGER_MARKER.set_lossy(
        txn.get_class_manager_block_marker().expect("Should have a class manager block marker").0,
    );
    SYNC_COMPILED_CLASS_MARKER
        .set_lossy(txn.get_compiled_class_marker().expect("Should have a compiled class marker").0);
}

fn reconstruct_processed_transactions_metric(txn: &StorageTxn<'_, impl TransactionKind>) {
    let block_marker = txn.get_body_marker().expect("Should have a body marker");

    for current_block_number in 0..block_marker.0 {
        let current_block_tx_count = txn
            .get_block_transactions_count(BlockNumber(current_block_number))
            .expect("Should have block transactions count")
            .expect("Missing block body with block number smaller than body marker");
        SYNC_PROCESSED_TRANSACTIONS
            .increment(current_block_tx_count.try_into().expect("Failed to convert usize to u64"));
    }
}

pub type StateSyncRunnerServer = WrapperServer<StateSyncRunner>;
// TODO(shahak): fill with a proper version, or allow not specifying the node version.
const VERSION_FULL: &str = "";
