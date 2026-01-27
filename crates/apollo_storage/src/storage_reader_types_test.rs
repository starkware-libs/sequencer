use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_proc_macros::unique_u16;
use axum::http::StatusCode;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use starknet_api::{contract_address, felt, storage_key};

use crate::state::StateStorageWriter;
use crate::storage_reader_server::ServerConfig;
use crate::storage_reader_server_test_utils::get_response;
use crate::storage_reader_types::{
    GenericStorageReaderServer,
    StorageReaderRequest,
    StorageReaderResponse,
};
use crate::test_utils::get_test_storage;

// Creates an `AvailablePorts` instance with a unique `instance_index`.
// Each test that binds ports should use a different instance_index to get disjoint port ranges.
// This is necessary to allow running tests concurrently in different processes, which do not have a
// shared memory.
fn available_ports_factory(instance_index: u16) -> AvailablePorts {
    AvailablePorts::new(TestIdentifier::StorageReaderTypesUnitTests.into(), instance_index)
}

#[tokio::test]
async fn state_diffs_requests() {
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

    let config = ServerConfig::new(
        IpAddr::from(Ipv4Addr::LOCALHOST),
        available_ports_factory(unique_u16!()).get_next_port(),
        true,
    );

    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    // Get the location of the state diff
    let expected_location = reader
        .begin_ro_txn()
        .unwrap()
        .get_state_diff_location(block_number)
        .unwrap()
        .expect("State diff location should exist");

    let location_request = StorageReaderRequest::StateDiffsLocation(block_number);
    let location_response: StorageReaderResponse =
        get_response(app.clone(), &location_request, StatusCode::OK).await;

    let location = match location_response {
        StorageReaderResponse::StateDiffsLocation(loc) => {
            assert_eq!(loc, expected_location);
            loc
        }
        _ => panic!("Expected StateDiffsLocation response"),
    };

    // Get the state diff using the location
    let state_diff_request = StorageReaderRequest::StateDiffsFromLocation(location);
    let state_diff_response: StorageReaderResponse =
        get_response(app, &state_diff_request, StatusCode::OK).await;

    assert_eq!(state_diff_response, StorageReaderResponse::StateDiffsFromLocation(test_state_diff));
}

#[tokio::test]
async fn contract_storage_request() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // TODO(Nadin): Create a test function for the setup.
    // Setup test data
    let block_number = BlockNumber(0);
    let contract_address = contract_address!("0x100");
    let storage_key = storage_key!("0x10");
    let storage_value = felt!("0x42");

    // Create state diff with storage data
    let storage_diffs = IndexMap::from([(storage_key, storage_value)]);
    let state_diff = ThinStateDiff {
        storage_diffs: IndexMap::from([(contract_address, storage_diffs)]),
        ..Default::default()
    };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block_number, state_diff)
        .unwrap()
        .commit()
        .unwrap();

    let config = ServerConfig::new(
        IpAddr::from(Ipv4Addr::LOCALHOST),
        available_ports_factory(unique_u16!()).get_next_port(),
        true,
    );
    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    // Request the storage value
    let request =
        StorageReaderRequest::ContractStorage((contract_address, storage_key), block_number);
    let response: StorageReaderResponse = get_response(app, &request, StatusCode::OK).await;

    assert_eq!(response, StorageReaderResponse::ContractStorage(storage_value));
}
