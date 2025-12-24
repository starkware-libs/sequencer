use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use axum::http::StatusCode;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;

use crate::state::StateStorageWriter;
use crate::storage_reader_server::ServerConfig;
use crate::storage_reader_server_test_utils::get_response;
use crate::storage_reader_types::{
    GenericStorageReaderServer,
    StorageReaderRequest,
    StorageReaderResponse,
};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn state_diff_location_request() {
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

    let mut available_ports =
        AvailablePorts::new(TestIdentifier::StorageReaderTypesUnitTests.into(), 0);

    let config =
        ServerConfig::new(IpAddr::from(Ipv4Addr::LOCALHOST), available_ports.get_next_port(), true);

    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    let expected_location = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_diff_location(block_number)
        .unwrap()
        .expect("State diff location should exist");

    let request = StorageReaderRequest::StateDiffLocation(block_number);
    let response: StorageReaderResponse = get_response(app, &request, StatusCode::OK).await;

    assert_eq!(response, StorageReaderResponse::StateDiffLocation(expected_location));
}
