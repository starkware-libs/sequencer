use std::net::{IpAddr, Ipv4Addr};

use apollo_storage::state::StateStorageWriter;
use apollo_storage::storage_reader_server::{ServerConfig, StorageReaderServer};
use apollo_storage::storage_reader_server_test_utils::get_response;
use apollo_storage::test_utils::get_test_storage;
use axum::http::StatusCode;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

use crate::batcher_types::{
    BatcherStorageReaderServerHandler,
    BatcherStorageRequest,
    BatcherStorageResponse,
};

#[tokio::test]
async fn test_batcher_storage_reader_server_handler() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Add a test state diff at block 0
    let block_number = BlockNumber(0);
    let test_state_diff = ThinStateDiff::default();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block_number, test_state_diff.clone())
        .unwrap()
        .commit()
        .unwrap();

    let config = ServerConfig::new(IpAddr::from(Ipv4Addr::LOCALHOST), 8081, true);

    let server = StorageReaderServer::<
        BatcherStorageReaderServerHandler,
        BatcherStorageRequest,
        BatcherStorageResponse,
    >::new(reader.clone(), config);
    let app = server.app();

    let expected_location = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_diff_location(block_number)
        .unwrap()
        .expect("State diff location should exist");

    let request = BatcherStorageRequest::StateDiffLocation(block_number);
    let batcher_response: BatcherStorageResponse =
        get_response(app, &request, StatusCode::OK).await;

    assert_eq!(batcher_response, BatcherStorageResponse::StateDiffLocation(expected_location));
}
