use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_proc_macros::unique_u16;
use axum::http::StatusCode;
use axum::Router;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
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

const TEST_BLOCK_NUMBER: BlockNumber = BlockNumber(0);

/// Test server setup containing all the components needed for testing storage reader requests.
struct TestServerSetup {
    app: Router,
    reader: StorageReader,
    state_diff: ThinStateDiff,
    _temp_dir: TempDir,
    class_hash: ClassHash,
    class: SierraContractClass,
    deprecated_class_hash: ClassHash,
    deprecated_class: DeprecatedContractClass,
}

impl TestServerSetup {
    async fn get_success_response<Req: Serialize, Res: DeserializeOwned>(
        &self,
        request: &Req,
    ) -> Res {
        get_response(self.app.clone(), request, StatusCode::OK).await
    }
}

// Creates an `AvailablePorts` instance with a unique `instance_index`.
// Each test that binds ports should use a different instance_index to get disjoint port ranges.
// This is necessary to allow running tests concurrently in different processes, which do not have a
// shared memory.
fn available_ports_factory(instance_index: u16) -> AvailablePorts {
    AvailablePorts::new(TestIdentifier::StorageReaderTypesUnitTests.into(), instance_index)
}

/// Sets up a test server with a comprehensive state diff at the specified block number.
fn setup_test_server(block_number: BlockNumber, instance_index: u16) -> TestServerSetup {
    let ((reader, mut writer), temp_dir) = get_test_storage();

    let contract_address = contract_address!("0x100");
    let storage_key = storage_key!("0x10");
    let storage_value = felt!("0x42");

    let storage_diffs = IndexMap::from([(storage_key, storage_value)]);

    let expected_class: SierraContractClass = read_json_file("class.json");
    let class_hash = expected_class.calculate_class_hash();

    let expected_deprecated_class: DeprecatedContractClass =
        read_json_file("deprecated_class.json");
    let deprecated_class_hash = ClassHash(felt!("0x1"));

    let state_diff = ThinStateDiff {
        storage_diffs: IndexMap::from([(contract_address, storage_diffs)]),
        class_hash_to_compiled_class_hash: IndexMap::from([(
            class_hash,
            compiled_class_hash!(1_u8),
        )]),
        deprecated_declared_classes: vec![deprecated_class_hash],
        ..Default::default()
    };

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block_number, state_diff.clone())
        .unwrap()
        .append_classes(
            block_number,
            &[(class_hash, &expected_class)],
            &[(deprecated_class_hash, &expected_deprecated_class)],
        )
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

    TestServerSetup {
        app,
        reader,
        state_diff,
        _temp_dir: temp_dir,
        class_hash,
        class: expected_class,
        deprecated_class_hash,
        deprecated_class: expected_deprecated_class,
    }
}

#[tokio::test]
async fn state_diff_location_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_location = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_state_diff_location(TEST_BLOCK_NUMBER)
        .unwrap()
        .expect("State diff location should exist");

    let location_request = StorageReaderRequest::StateDiffsLocation(TEST_BLOCK_NUMBER);
    let location_response: StorageReaderResponse =
        setup.get_success_response(&location_request).await;

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
        setup.get_success_response(&state_diff_request).await;

    assert_eq!(
        state_diff_response,
        StorageReaderResponse::StateDiffsFromLocation(setup.state_diff)
    );
}

#[tokio::test]
async fn contract_storage_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Extract the test data from the state diff
    let (contract_address, storage_diffs) = setup.state_diff.storage_diffs.iter().next().unwrap();
    let (storage_key, storage_value) = storage_diffs.iter().next().unwrap();

    // Request the storage value
    let request =
        StorageReaderRequest::ContractStorage((*contract_address, *storage_key), TEST_BLOCK_NUMBER);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::ContractStorage(*storage_value));
}

#[tokio::test]
async fn declared_class_requests() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Get the expected location
    let expected_location = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_class_location(&setup.class_hash)
        .unwrap()
        .expect("Class location should exist");

    // Test DeclaredClassesLocation request
    let location_request = StorageReaderRequest::DeclaredClassesLocation(setup.class_hash);
    let location_response: StorageReaderResponse =
        setup.get_success_response(&location_request).await;

    let location = match location_response {
        StorageReaderResponse::DeclaredClassesLocation(loc) => {
            assert_eq!(loc, expected_location);
            loc
        }
        _ => panic!("Expected DeclaredClassesLocation response"),
    };

    // Test DeclaredClassesFromLocation request
    let class_request = StorageReaderRequest::DeclaredClassesFromLocation(location);
    let class_response: StorageReaderResponse = setup.get_success_response(&class_request).await;

    assert_eq!(class_response, StorageReaderResponse::DeclaredClassesFromLocation(setup.class));
}

#[tokio::test]
async fn declared_class_block_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let request = StorageReaderRequest::DeclaredClassesBlock(setup.class_hash);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::DeclaredClassesBlock(TEST_BLOCK_NUMBER));
}

#[tokio::test]
async fn deprecated_declared_class_requests() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Get the expected location
    let expected_location = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_deprecated_class_location(&setup.deprecated_class_hash)
        .unwrap()
        .expect("Deprecated class location should exist");

    // Test DeprecatedDeclaredClassesLocation request
    let location_request =
        StorageReaderRequest::DeprecatedDeclaredClassesLocation(setup.deprecated_class_hash);
    let location_response: StorageReaderResponse =
        get_response(setup.app.clone(), &location_request, StatusCode::OK).await;

    let location = match location_response {
        StorageReaderResponse::DeprecatedDeclaredClassesLocation(loc) => {
            assert_eq!(loc, expected_location);
            loc
        }
        _ => panic!("Expected DeprecatedDeclaredClassesLocation response"),
    };

    // Test DeprecatedDeclaredClassesFromLocation request
    let class_request = StorageReaderRequest::DeprecatedDeclaredClassesFromLocation(location);
    let class_response: StorageReaderResponse =
        get_response(setup.app, &class_request, StatusCode::OK).await;

    assert_eq!(
        class_response,
        StorageReaderResponse::DeprecatedDeclaredClassesFromLocation(setup.deprecated_class)
    );
}
