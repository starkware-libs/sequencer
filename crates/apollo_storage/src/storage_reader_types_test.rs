use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Mutex, OnceLock};

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

// Shared port allocator for all tests to ensure unique ports across parallel test execution
static AVAILABLE_PORTS: OnceLock<Mutex<AvailablePorts>> = OnceLock::new();

fn get_next_test_port() -> u16 {
    AVAILABLE_PORTS
        .get_or_init(|| {
            Mutex::new(AvailablePorts::new(TestIdentifier::StorageReaderTypesUnitTests.into(), 0))
        })
        .lock()
        .unwrap()
        .get_next_port()
}

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

    let config = ServerConfig::new(IpAddr::from(Ipv4Addr::LOCALHOST), get_next_test_port(), true);

    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    let expected_location = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_diff_location(block_number)
        .unwrap()
        .expect("State diff location should exist");

    let request = StorageReaderRequest::StateDiffsLocation(block_number);
    let response: StorageReaderResponse = get_response(app, &request, StatusCode::OK).await;

    assert_eq!(response, StorageReaderResponse::StateDiffsLocation(expected_location));
}
