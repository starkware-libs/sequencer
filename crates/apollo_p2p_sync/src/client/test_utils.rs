use core::panic;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use apollo_class_manager_types::MockClassManagerClient;
use apollo_network::network_manager::test_utils::{
    mock_register_sqmr_protocol_client,
    MockClientResponsesManager,
};
use apollo_network::network_manager::GenericReceiver;
use apollo_protobuf::sync::{
    ClassQuery,
    DataOrFin,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::StorageReader;
use apollo_test_utils::GetTestInstance;
use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::{FutureExt, SinkExt, StreamExt};
use lazy_static::lazy_static;
use papyrus_common::pending_classes::ApiContractClass;
use rand::{Rng, RngCore};
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockSignature,
};
use starknet_api::core::ClassHash;
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::FullTransaction;
use starknet_types_core::felt::Felt;
use tokio::sync::oneshot;

use super::{P2pSyncClient, P2pSyncClientChannels, P2pSyncClientConfig};

pub(crate) const TIMEOUT_FOR_TEST: Duration = Duration::from_secs(5);
pub const BUFFER_SIZE: usize = 1000;
pub const HEADER_QUERY_LENGTH: u64 = 5;
pub const STATE_DIFF_QUERY_LENGTH: u64 = 3;
pub const CLASS_DIFF_QUERY_LENGTH: u64 = 3;
pub const TRANSACTION_QUERY_LENGTH: u64 = 3;
pub const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
pub const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_secs(1);
pub const WAIT_PERIOD_FOR_OTHER_PROTOCOL: Duration = Duration::from_secs(1);
pub const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(Duration::from_secs(1));

lazy_static! {
    static ref TEST_CONFIG: P2pSyncClientConfig = P2pSyncClientConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        num_block_transactions_per_query: TRANSACTION_QUERY_LENGTH,
        num_block_classes_per_query: CLASS_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        wait_period_for_other_protocol: WAIT_PERIOD_FOR_OTHER_PROTOCOL,
        buffer_size: BUFFER_SIZE,
    };
}
pub(crate) type HeaderTestPayload =
    MockClientResponsesManager<HeaderQuery, DataOrFin<SignedBlockHeader>>;
pub(crate) type StateDiffTestPayload =
    MockClientResponsesManager<StateDiffQuery, DataOrFin<StateDiffChunk>>;
pub(crate) type TransactionTestPayload =
    MockClientResponsesManager<TransactionQuery, DataOrFin<FullTransaction>>;
pub(crate) type ClassTestPayload =
    MockClientResponsesManager<ClassQuery, DataOrFin<(ApiContractClass, ClassHash)>>;

// TODO(Eitan): Use SqmrSubscriberChannels once there is a utility function for testing
pub struct TestArgs {
    #[allow(clippy::type_complexity)]
    pub p2p_sync: P2pSyncClient,
    pub storage_reader: StorageReader,
    pub mock_header_response_manager: GenericReceiver<HeaderTestPayload>,
    pub mock_state_diff_response_manager: GenericReceiver<StateDiffTestPayload>,
    #[allow(dead_code)]
    pub mock_transaction_response_manager: GenericReceiver<TransactionTestPayload>,
    #[allow(dead_code)]
    pub mock_class_response_manager: GenericReceiver<ClassTestPayload>,
}

pub fn setup() -> TestArgs {
    let p2p_sync_config = *TEST_CONFIG;
    let buffer_size = p2p_sync_config.buffer_size;
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_sender, mock_header_response_manager) =
        mock_register_sqmr_protocol_client(buffer_size);
    let (state_diff_sender, mock_state_diff_response_manager) =
        mock_register_sqmr_protocol_client(buffer_size);
    let (transaction_sender, mock_transaction_response_manager) =
        mock_register_sqmr_protocol_client(buffer_size);
    let (class_sender, mock_class_response_manager) =
        mock_register_sqmr_protocol_client(buffer_size);
    let p2p_sync_channels = P2pSyncClientChannels {
        header_sender,
        state_diff_sender,
        transaction_sender,
        class_sender,
    };
    let class_manager_client = Arc::new(MockClassManagerClient::new());
    let p2p_sync = P2pSyncClient::new(
        p2p_sync_config,
        storage_reader.clone(),
        storage_writer,
        p2p_sync_channels,
        futures::stream::pending().boxed(),
        class_manager_client,
    );
    TestArgs {
        p2p_sync,
        storage_reader,
        mock_header_response_manager,
        mock_state_diff_response_manager,
        mock_transaction_response_manager,
        mock_class_response_manager,
    }
}

#[derive(Eq, PartialEq, Hash)]
pub enum DataType {
    Header,
    #[allow(dead_code)]
    Transaction,
    StateDiff,
    #[allow(dead_code)]
    Class,
}

pub enum Action {
    /// Run the p2p sync client.
    RunP2pSync,
    /// Get a header query from the sync and run custom validations on it.
    ReceiveQuery(Box<dyn FnOnce(Query)>, DataType),
    /// Send a header as a response to a query we got from ReceiveQuery. Will panic if didn't call
    /// ReceiveQuery with DataType::Header before.
    SendHeader(DataOrFin<SignedBlockHeader>),
    /// Send a state diff as a response to a query we got from ReceiveQuery. Will panic if didn't
    /// call ReceiveQuery with DataType::StateDiff before.
    SendStateDiff(DataOrFin<StateDiffChunk>),
    /// Send a transaction as a response to a query we got from ReceiveQuery. Will panic if didn't
    /// call ReceiveQuery with DataType::Transaction before.
    SendTransaction(DataOrFin<FullTransaction>),
    /// Send a class as a response to a query we got from ReceiveQuery. Will panic if didn't
    /// call ReceiveQuery with DataType::Class before.
    SendClass(DataOrFin<(ApiContractClass, ClassHash)>),
    /// Perform custom validations on the storage. Returns back the storage reader it received as
    /// input
    CheckStorage(Box<dyn FnOnce(StorageReader) -> BoxFuture<'static, ()>>),
    /// Check that a report was sent on the current header query.
    ValidateReportSent(DataType),
    /// Sends an internal block to the sync.
    #[allow(dead_code)]
    SendInternalBlock(SyncBlock),
    /// Sleep for SLEEP_DURATION_TO_LET_SYNC_ADVANCE duration.
    SleepToLetSyncAdvance,
    /// Simulate WAIT_PERIOD_FOR_OTHER_PROTOCOL duration has passed.
    SimulateWaitPeriodForOtherProtocol,
}

// TODO(shahak): add support for state diffs, transactions and classes.
pub async fn run_test(
    max_query_lengths: HashMap<DataType, u64>,
    class_manager_client: Option<MockClassManagerClient>,
    actions: Vec<Action>,
) {
    let p2p_sync_config = P2pSyncClientConfig {
        num_headers_per_query: max_query_lengths.get(&DataType::Header).cloned().unwrap_or(1),
        num_block_state_diffs_per_query: max_query_lengths
            .get(&DataType::StateDiff)
            .cloned()
            .unwrap_or(1),
        num_block_transactions_per_query: max_query_lengths
            .get(&DataType::Transaction)
            .cloned()
            .unwrap_or(1),
        num_block_classes_per_query: max_query_lengths.get(&DataType::Class).cloned().unwrap_or(1),
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        wait_period_for_other_protocol: WAIT_PERIOD_FOR_OTHER_PROTOCOL,
        buffer_size: BUFFER_SIZE,
    };
    let class_manager_client = class_manager_client.unwrap_or_default();
    let class_manager_client = Arc::new(class_manager_client);
    let buffer_size = p2p_sync_config.buffer_size;
    let ((storage_reader, storage_writer), _temp_dir) = get_test_storage();
    let (header_sender, mut mock_header_network) = mock_register_sqmr_protocol_client(buffer_size);
    let (state_diff_sender, mut mock_state_diff_network) =
        mock_register_sqmr_protocol_client(buffer_size);
    let (transaction_sender, mut mock_transaction_network) =
        mock_register_sqmr_protocol_client(buffer_size);
    let (class_sender, mut mock_class_network) = mock_register_sqmr_protocol_client(buffer_size);
    let p2p_sync_channels = P2pSyncClientChannels {
        header_sender,
        state_diff_sender,
        transaction_sender,
        class_sender,
    };
    let (mut internal_block_sender, internal_block_receiver) = mpsc::channel(buffer_size);
    let p2p_sync = P2pSyncClient::new(
        p2p_sync_config,
        storage_reader.clone(),
        storage_writer,
        p2p_sync_channels,
        internal_block_receiver.boxed(),
        class_manager_client,
    );

    let mut headers_current_query_responses_manager = None;
    let mut state_diff_current_query_responses_manager = None;
    let mut transaction_current_query_responses_manager = None;
    let mut class_current_query_responses_manager = None;

    let (sync_future_sender, sync_future_receiver) = oneshot::channel();
    let mut sync_future_sender = Some(sync_future_sender);

    tokio::select! {
        _ = async {
            for action in actions {
                match action {
                    Action::ReceiveQuery(validate_query_fn, data_type) => {
                        let query = match data_type {
                            DataType::Header => {
                                get_next_query_and_update_responses_manager(
                                    &mut mock_header_network,
                                    &mut headers_current_query_responses_manager,
                                ).await.0
                            }
                            DataType::StateDiff => {
                                get_next_query_and_update_responses_manager(
                                    &mut mock_state_diff_network,
                                    &mut state_diff_current_query_responses_manager,
                                ).await.0
                            }
                            DataType::Transaction => {
                                get_next_query_and_update_responses_manager(
                                    &mut mock_transaction_network,
                                    &mut transaction_current_query_responses_manager,
                                ).await.0
                            }
                            DataType::Class => {
                                get_next_query_and_update_responses_manager(
                                    &mut mock_class_network,
                                    &mut class_current_query_responses_manager,
                                ).await.0
                            }
                        };
                        validate_query_fn(query);
                    }
                    Action::SendHeader(header_or_fin) => {
                        let responses_manager = headers_current_query_responses_manager.as_mut()
                            .expect("Called SendHeader without calling ReceiveQuery");
                        responses_manager.send_response(header_or_fin).await.unwrap();
                    }
                    Action::SendStateDiff(state_diff_or_fin) => {
                        let responses_manager = state_diff_current_query_responses_manager.as_mut()
                            .expect("Called SendStateDiff without calling ReceiveQuery");
                        responses_manager.send_response(state_diff_or_fin).await.unwrap();
                    }
                    Action::SendTransaction(transaction_or_fin) => {
                        let responses_manager = transaction_current_query_responses_manager.as_mut()
                            .expect("Called SendTransaction without calling ReceiveQuery");
                        responses_manager.send_response(transaction_or_fin).await.unwrap();
                    }
                    Action::SendClass(class_or_fin) => {
                        let responses_manager = class_current_query_responses_manager.as_mut()
                            .expect("Called SendClass without calling ReceiveQuery");
                        responses_manager.send_response(class_or_fin).await.unwrap();
                    }
                    Action::CheckStorage(check_storage_fn) => {
                        // We tried avoiding the clone here but it causes lifetime issues.
                        check_storage_fn(storage_reader.clone()).await;
                    }
                    Action::ValidateReportSent(DataType::Header) => {
                        let responses_manager = headers_current_query_responses_manager.take()
                            .expect(
                                "Called ValidateReportSent without calling ReceiveQuery on the same
                                data type");
                        responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
                    }
                    Action::ValidateReportSent(DataType::StateDiff) => {
                        let responses_manager = state_diff_current_query_responses_manager.take()
                            .expect(
                                "Called ValidateReportSent without calling ReceiveQuery on the same
                                data type");
                        responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
                    }
                    Action::ValidateReportSent(DataType::Transaction) => {
                        let responses_manager = transaction_current_query_responses_manager.take()
                            .expect(
                                "Called ValidateReportSent without calling ReceiveQuery on the same
                                data type");
                        responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
                    }
                    Action::ValidateReportSent(DataType::Class) => {
                        let responses_manager = class_current_query_responses_manager.take()
                            .expect(
                                "Called ValidateReportSent without calling ReceiveQuery on the same
                                data type");
                        responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
                    }
                    Action::SendInternalBlock(sync_block) => {
                        internal_block_sender.send(sync_block).await.unwrap();
                    }
                    Action::RunP2pSync => {
                        sync_future_sender.take().expect("Called RunP2pSync twice").send(()).expect("Failed to send message to run p2p sync");
                    }
                    Action::SleepToLetSyncAdvance => {
                        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;
                    }
                    Action::SimulateWaitPeriodForOtherProtocol => {
                        tokio::time::pause();
                        tokio::time::advance(WAIT_PERIOD_FOR_OTHER_PROTOCOL).await;
                        tokio::time::resume();
                    }
                }
            }
        } => {},
        res = sync_future_receiver.then(|res| async {
            res.expect("Failed to run p2p sync");
            p2p_sync.run().await
        }) => {
            res.unwrap();
            // If return type is no longer Never we still want the unreachable! check.
            #[allow(unreachable_code)]
            {
                unreachable!("Return type Never should never be constructed.");
            }
        }
        _ = tokio::time::sleep(TIMEOUT_FOR_TEST) => {
            panic!("Test timed out.");
        }
    }
}

pub fn random_header(
    rng: &mut ChaCha8Rng,
    block_number: BlockNumber,
    state_diff_length: Option<usize>,
    num_transactions: Option<usize>,
) -> SignedBlockHeader {
    SignedBlockHeader {
        block_header: BlockHeader {
            // TODO(shahak): Remove this once get_test_instance puts random values.
            block_hash: BlockHash(rng.next_u64().into()),
            block_header_without_hash: BlockHeaderWithoutHash {
                block_number,
                ..GetTestInstance::get_test_instance(rng)
            },
            state_diff_length: Some(state_diff_length.unwrap_or_else(|| rng.gen())),
            n_transactions: num_transactions.unwrap_or_else(|| rng.gen()),
            ..GetTestInstance::get_test_instance(rng)
        },
        // TODO(shahak): Remove this once get_test_instance puts random values.
        signatures: vec![BlockSignature(Signature {
            r: rng.next_u64().into(),
            s: rng.next_u64().into(),
        })],
    }
}

pub fn create_block_hashes_and_signatures(n_blocks: u8) -> Vec<(BlockHash, BlockSignature)> {
    let mut bytes = [0u8; 32];
    (0u8..n_blocks)
        .map(|i| {
            bytes[31] = i;
            (
                BlockHash(StarkHash::from_bytes_be(&bytes)),
                BlockSignature(Signature {
                    r: Felt::from_bytes_be(&bytes),
                    s: Felt::from_bytes_be(&bytes),
                }),
            )
        })
        .collect()
}

// TODO(Shahak): Consider moving this to storage and to use poll wakeup instead of sleep
pub(crate) async fn wait_for_marker(
    data_type: DataType,
    storage_reader: &StorageReader,
    expected_marker: BlockNumber,
    sleep_duration: Duration,
    timeout: Duration,
) {
    let start_time = Instant::now();

    loop {
        assert!(start_time.elapsed() < timeout, "Timeout waiting for marker");

        let txn = storage_reader.begin_ro_txn().unwrap();
        let storage_marker = match data_type {
            DataType::Header => txn.get_header_marker().unwrap(),
            DataType::Transaction => txn.get_body_marker().unwrap(),
            DataType::StateDiff => txn.get_state_marker().unwrap(),
            DataType::Class => txn.get_class_manager_block_marker().unwrap(),
        };

        if storage_marker >= expected_marker {
            break;
        }

        tokio::time::sleep(sleep_duration).await;
    }
}

async fn get_next_query_and_update_responses_manager<
    Query: TryFrom<Vec<u8>> + Clone,
    Response: TryFrom<Vec<u8>>,
>(
    mock_network: &mut GenericReceiver<MockClientResponsesManager<Query, Response>>,
    current_query_responses_manager: &mut Option<MockClientResponsesManager<Query, Response>>,
) -> Query
where
    <Query as TryFrom<Vec<u8>>>::Error: Debug,
{
    let responses_manager = mock_network.next().await.unwrap();
    let query = responses_manager.query().as_ref().unwrap().clone();
    *current_query_responses_manager = Some(responses_manager);
    query
}
