use std::net::Ipv4Addr;

use apollo_storage::state::StateStorageWriter;
use apollo_storage::storage_reader_server::{ServerConfig, StorageReaderServer};
use apollo_storage::storage_reader_server_test_utils::{send_storage_query, to_bytes};
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

    let config = ServerConfig::new(Ipv4Addr::LOCALHOST, 8081, true);

    let server = StorageReaderServer::<
        BatcherStorageReaderServerHandler,
        BatcherStorageRequest,
        BatcherStorageResponse,
    >::new(reader.clone(), config);
    let app = server.app();

    let request = BatcherStorageRequest::StateDiffByBlockNumber(BlockNumber(0));
    let response = send_storage_query(app, &request).await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response).await;
    let batcher_response: BatcherStorageResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(batcher_response, BatcherStorageResponse::StateDiffByBlockNumber(test_state_diff));
}
