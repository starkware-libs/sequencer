use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
// TODO(victork): finalise migration to hyper 1.x
use apollo_proc_macros::unique_u16;
use axum_08::http::StatusCode;
use axum_08::Router;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::{SierraContractClass, ThinStateDiff};
use starknet_api::test_utils::read_json_file;
use starknet_api::{compiled_class_hash, contract_address, felt, storage_key};
use tempfile::TempDir;

use crate::class::{ClassStorageReader, ClassStorageWriter};
use crate::state::StateStorageWriter;
use crate::storage_reader_server::ServerConfig;
use crate::storage_reader_server_test_utils::get_response;
use crate::storage_reader_types::{
    GenericStorageReaderServer,
    StorageReaderRequest,
    StorageReaderResponse,
};
use crate::test_utils::get_test_storage;
use crate::StorageReader;

// Creates an `AvailablePorts` instance with a unique `instance_index`.
// Each test that binds ports should use a different instance_index to get disjoint port ranges.
// This is necessary to allow running tests concurrently in different processes, which do not have a
// shared memory.
fn available_ports_factory(instance_index: u16) -> AvailablePorts {
    AvailablePorts::new(TestIdentifier::StorageReaderTypesUnitTests.into(), instance_index)
}

/// Sets up a test server with a comprehensive state diff at the specified block number.
fn setup_test_server(
    block_number: BlockNumber,
    instance_index: u16,
) -> (Router, StorageReader, ThinStateDiff, TempDir, (ClassHash, SierraContractClass)) {
    let ((reader, mut writer), temp_dir) = get_test_storage();

    let contract_address = contract_address!("0x100");
    let storage_key = storage_key!("0x10");
    let storage_value = felt!("0x42");

    let storage_diffs = IndexMap::from([(storage_key, storage_value)]);

    let expected_class: SierraContractClass = read_json_file("class.json");
    let class_hash = expected_class.calculate_class_hash();

    let state_diff = ThinStateDiff {
        storage_diffs: IndexMap::from([(contract_address, storage_diffs)]),
        class_hash_to_compiled_class_hash: IndexMap::from([(
            class_hash,
            compiled_class_hash!(1_u8),
        )]),
        ..Default::default()
    };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block_number, state_diff.clone())
        .unwrap()
        .append_classes(block_number, &[(class_hash, &expected_class)], &[])
        .unwrap()
        .commit()
        .unwrap();

    let config = ServerConfig::new(
        IpAddr::from(Ipv4Addr::LOCALHOST),
        available_ports_factory(instance_index).get_next_port(),
        true,
    );
    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    (app, reader, state_diff, temp_dir, (class_hash, expected_class))
}

#[tokio::test]
async fn state_diff_location_request() {
    let block_number = BlockNumber(0);
    let (app, reader, state_diff, _temp_dir, _) = setup_test_server(block_number, unique_u16!());

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

    assert_eq!(state_diff_response, StorageReaderResponse::StateDiffsFromLocation(state_diff));
}

#[tokio::test]
async fn contract_storage_request() {
    let block_number = BlockNumber(0);
    let (app, _reader, state_diff, _temp_dir, _) = setup_test_server(block_number, unique_u16!());

    // Extract the test data from the state diff
    let (contract_address, storage_diffs) = state_diff.storage_diffs.iter().next().unwrap();
    let (storage_key, storage_value) = storage_diffs.iter().next().unwrap();

    // Request the storage value
    let request =
        StorageReaderRequest::ContractStorage((*contract_address, *storage_key), block_number);
    let response: StorageReaderResponse = get_response(app, &request, StatusCode::OK).await;

    assert_eq!(response, StorageReaderResponse::ContractStorage(*storage_value));
}

#[tokio::test]
async fn declared_class_requests() {
    let block_number = BlockNumber(0);
    let (app, reader, _state_diff, _temp_dir, (class_hash, expected_class)) =
        setup_test_server(block_number, unique_u16!());

    // Get the expected location
    let expected_location = reader
        .begin_ro_txn()
        .unwrap()
        .get_class_location(&class_hash)
        .unwrap()
        .expect("Class location should exist");

    // Test DeclaredClassesLocation request
    let location_request = StorageReaderRequest::DeclaredClassesLocation(class_hash);
    let location_response: StorageReaderResponse =
        get_response(app.clone(), &location_request, StatusCode::OK).await;

    let location = match location_response {
        StorageReaderResponse::DeclaredClassesLocation(loc) => {
            assert_eq!(loc, expected_location);
            loc
        }
        _ => panic!("Expected DeclaredClassesLocation response"),
    };

    // Test DeclaredClassesFromLocation request
    let class_request = StorageReaderRequest::DeclaredClassesFromLocation(location);
    let class_response: StorageReaderResponse =
        get_response(app, &class_request, StatusCode::OK).await;

    assert_eq!(class_response, StorageReaderResponse::DeclaredClassesFromLocation(expected_class));
}
