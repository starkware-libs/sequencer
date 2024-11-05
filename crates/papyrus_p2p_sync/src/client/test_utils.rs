use std::time::Duration;

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
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageReader;
use starknet_api::block::{BlockHash, BlockSignature};
use starknet_api::core::ClassHash;
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::FullTransaction;
use starknet_types_core::felt::Felt;

use super::{P2PSyncClient, P2PSyncClientChannels, P2PSyncClientConfig};

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
        num_transactions_per_query: TRANSACTION_QUERY_LENGTH,
        num_classes_per_query: CLASS_DIFF_QUERY_LENGTH,
        wait_period_for_new_data: WAIT_PERIOD_FOR_NEW_DATA,
        buffer_size: BUFFER_SIZE,
        stop_sync_at_block_number: None,
    };
}
type HeaderTestPayload = MockClientResponsesManager<HeaderQuery, DataOrFin<SignedBlockHeader>>;
type StateDiffTestPayload = MockClientResponsesManager<StateDiffQuery, DataOrFin<StateDiffChunk>>;
type TransactionTestPayload =
    MockClientResponsesManager<TransactionQuery, DataOrFin<FullTransaction>>;
type ClassTestPayload =
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
