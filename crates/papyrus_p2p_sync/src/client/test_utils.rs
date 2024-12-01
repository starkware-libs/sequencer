use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::StreamExt;
use lazy_static::lazy_static;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_network::network_manager::test_utils::{
    mock_register_sqmr_protocol_client,
    MockClientResponsesManager,
};
use papyrus_network::network_manager::GenericReceiver;
use papyrus_protobuf::sync::{
    ClassQuery,
    DataOrFin,
    HeaderQuery,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::class::ClassStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockNumber, BlockSignature};
use starknet_api::core::ClassHash;
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::FullTransaction;
use starknet_types_core::felt::Felt;

use super::{P2PSyncClient, P2PSyncClientChannels, P2PSyncClientConfig};

pub(crate) const TIMEOUT_FOR_TEST: Duration = Duration::from_secs(5);
pub const BUFFER_SIZE: usize = 1000;
pub const HEADER_QUERY_LENGTH: u64 = 5;
pub const STATE_DIFF_QUERY_LENGTH: u64 = 3;
pub const CLASS_DIFF_QUERY_LENGTH: u64 = 3;
pub const TRANSACTION_QUERY_LENGTH: u64 = 3;
pub const SLEEP_DURATION_TO_LET_SYNC_ADVANCE: Duration = Duration::from_millis(10);
pub const WAIT_PERIOD_FOR_NEW_DATA: Duration = Duration::from_secs(1);
pub const TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE: Duration =
    WAIT_PERIOD_FOR_NEW_DATA.saturating_add(Duration::from_secs(1));

lazy_static! {
    static ref TEST_CONFIG: P2PSyncClientConfig = P2PSyncClientConfig {
        num_headers_per_query: HEADER_QUERY_LENGTH,
        num_block_state_diffs_per_query: STATE_DIFF_QUERY_LENGTH,
        num_block_transactions_per_query: TRANSACTION_QUERY_LENGTH,
        num_block_classes_per_query: CLASS_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        buffer_size: BUFFER_SIZE,
        stop_sync_at_block_number: None,
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
    pub p2p_sync: P2PSyncClient,
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
    let p2p_sync_channels = P2PSyncClientChannels {
        header_sender,
        state_diff_sender,
        transaction_sender,
        class_sender,
    };
    let p2p_sync = P2PSyncClient::new(
        p2p_sync_config,
        storage_reader.clone(),
        storage_writer,
        p2p_sync_channels,
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

pub enum Action {
    /// Get a header query from the sync and run custom validations on it.
    ReceiveQuery(Box<dyn FnOnce(Query)>),
    /// Send a header as a response to a query we got from ReceiveQuery. Will panic if didn't call
    /// ReceiveQuery.
    SendHeader(DataOrFin<SignedBlockHeader>),
    /// Perform custom validations on the storage. Returns back the storage reader it received as
    /// input
    CheckStorage(Box<dyn FnOnce(StorageReader) -> BoxFuture<'static, ()>>),
    /// Check that a report was sent on the current header query.
    ValidateReportSent,
}

// TODO(shahak): add support for state diffs, transactions and classes.
// TODO(shahak): add max query size as argument instead of constant.
pub async fn run_test(actions: Vec<Action>) {
    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_header_response_manager,
        // The test will fail if we drop these
        mock_state_diff_response_manager: _mock_state_diff_responses_manager,
        mock_class_response_manager: _mock_class_responses_manager,
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        ..
    } = setup();

    let mut headers_current_query_responses_manager = None;

    tokio::select! {
        _ = async {
            for action in actions {
                match action {
                    Action::ReceiveQuery(validate_query_fn) => {
                        let responses_manager =
                            mock_header_response_manager.next().await.unwrap();
                        let query = responses_manager.query().as_ref().unwrap().0.clone();
                        validate_query_fn(query);
                        headers_current_query_responses_manager = Some(responses_manager);
                    }
                    Action::SendHeader(header_or_fin) => {
                        let responses_manager = headers_current_query_responses_manager.as_mut()
                            .expect("Called SendHeader without calling ReceiveQuery");
                        responses_manager.send_response(header_or_fin).await.unwrap();
                    }
                    Action::CheckStorage(check_storage_fn) => {
                        // We tried avoiding the clone here but it causes lifetime issues.
                        check_storage_fn(storage_reader.clone()).await;
                    }
                    Action::ValidateReportSent => {
                        let responses_manager = headers_current_query_responses_manager.take()
                            .expect("Called ValidateReportSent without calling ReceiveQuery");
                        responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
                    }
                }
            }
        } => {},
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = tokio::time::sleep(TIMEOUT_FOR_TEST) => {
            panic!("Test timed out.");
        }
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

pub(crate) enum MarkerKind {
    Header,
    #[allow(dead_code)]
    Body,
    State,
    #[allow(dead_code)]
    Class,
}

// TODO: Consider moving this to storage and to use poll wakeup instead of sleep
pub(crate) async fn wait_for_marker(
    marker_kind: MarkerKind,
    storage_reader: &StorageReader,
    expected_marker: BlockNumber,
    sleep_duration: Duration,
    timeout: Duration,
) {
    let start_time = Instant::now();

    loop {
        assert!(start_time.elapsed() < timeout, "Timeout waiting for marker");

        let txn = storage_reader.begin_ro_txn().unwrap();
        let storage_marker = match marker_kind {
            MarkerKind::Header => txn.get_header_marker().unwrap(),
            MarkerKind::Body => txn.get_body_marker().unwrap(),
            MarkerKind::State => txn.get_state_marker().unwrap(),
            MarkerKind::Class => txn.get_class_marker().unwrap(),
        };

        if storage_marker >= expected_marker {
            break;
        }

        tokio::time::sleep(sleep_duration).await;
    }
}
