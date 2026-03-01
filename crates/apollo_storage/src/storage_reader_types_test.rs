use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_proc_macros::unique_u16;
use apollo_test_utils::get_test_block;
use axum::http::StatusCode;
use axum::Router;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use starknet_api::block::{BlockNumber, BlockSignature};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, ThinStateDiff};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::TransactionOffsetInBlock;
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, nonce, storage_key};
use tempfile::TempDir;

use crate::base_layer::BaseLayerStorageReader;
use crate::body::{BodyStorageReader, BodyStorageWriter, TransactionIndex};
use crate::class::{ClassStorageReader, ClassStorageWriter};
use crate::class_hash::ClassHashStorageWriter;
use crate::class_manager::ClassManagerStorageReader;
use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::consensus::{ConsensusStorageWriter, LastVotedMarker};
use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::state::{StateStorageReader, StateStorageWriter};
use crate::storage_reader_server::ServerConfig;
use crate::storage_reader_server_test_utils::get_response;
use crate::storage_reader_types::{
    GenericStorageReaderServer,
    StorageReaderRequest,
    StorageReaderResponse,
};
use crate::test_utils::get_test_storage;
use crate::version::VersionStorageReader;
use crate::{MarkerKind, OffsetKind, StorageReader};

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
    casm_class_hash: ClassHash,
    casm: CasmContractClass,
    tx_index: TransactionIndex,
    executable_class_hash_v2: CompiledClassHash,
    last_voted_marker: LastVotedMarker,
    block_signature: BlockSignature,
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
    let nonce = nonce!(0x5);

    let deployed_contract_address = contract_address!("0x200");
    let deployed_class_hash = class_hash!("0x1234");

    let storage_diffs = IndexMap::from([(storage_key, storage_value)]);

    let expected_class: SierraContractClass = read_json_file("class.json");
    let class_hash = expected_class.calculate_class_hash();

    let expected_deprecated_class: DeprecatedContractClass =
        read_json_file("deprecated_class.json");
    let deprecated_class_hash = ClassHash(felt!("0x1"));

    let expected_casm: CasmContractClass = read_json_file("compiled_class.json");
    let casm_class_hash = ClassHash(felt!("0x2"));
    let executable_class_hash_v2 = compiled_class_hash!(2_u8);
    let last_voted_marker = LastVotedMarker { height: block_number };

    let state_diff = ThinStateDiff {
        deployed_contracts: IndexMap::from([(deployed_contract_address, deployed_class_hash)]),
        storage_diffs: IndexMap::from([(contract_address, storage_diffs)]),
        class_hash_to_compiled_class_hash: IndexMap::from([(
            class_hash,
            compiled_class_hash!(1_u8),
        )]),
        deprecated_declared_classes: vec![deprecated_class_hash],
        nonces: IndexMap::from([(contract_address, nonce)]),
    };

    // Create a test block with transactions
    let block = get_test_block(3, Some(1), None, None);
    let tx_index = TransactionIndex(block_number, TransactionOffsetInBlock(0));
    let block_signature = BlockSignature::default();

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
        .append_casm(&casm_class_hash, &expected_casm)
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_block_signature(block_number, &block_signature)
        .unwrap()
        .append_body(block_number, block.body)
        .unwrap()
        .set_executable_class_hash_v2(&class_hash, executable_class_hash_v2)
        .unwrap()
        .set_last_voted_marker(&last_voted_marker)
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
        casm_class_hash,
        casm: expected_casm,
        tx_index,
        executable_class_hash_v2,
        last_voted_marker,
        block_signature,
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

#[tokio::test]
async fn casm_requests() {
    let block_number = BlockNumber(0);
    let setup = setup_test_server(block_number, unique_u16!());

    // Get the expected location
    let expected_location = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_casm_location(&setup.casm_class_hash)
        .unwrap()
        .expect("CASM location should exist");

    // Test CasmsLocation request
    let location_request = StorageReaderRequest::CasmsLocation(setup.casm_class_hash);
    let location_response: StorageReaderResponse =
        get_response(setup.app.clone(), &location_request, StatusCode::OK).await;

    let location = match location_response {
        StorageReaderResponse::CasmsLocation(loc) => {
            assert_eq!(loc, expected_location);
            loc
        }
        _ => panic!("Expected CasmsLocation response"),
    };

    // Test CasmsFromLocation request
    let casm_request = StorageReaderRequest::CasmsFromLocation(location);
    let casm_response: StorageReaderResponse =
        get_response(setup.app, &casm_request, StatusCode::OK).await;

    assert_eq!(casm_response, StorageReaderResponse::CasmsFromLocation(setup.casm));
}

#[tokio::test]
async fn transaction_metadata_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Get the expected transaction metadata directly from storage
    let expected_metadata = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_transaction_metadata(&setup.tx_index)
        .unwrap()
        .expect("Transaction metadata should exist");

    // Test TransactionMetadata request
    let request = StorageReaderRequest::TransactionMetadata(setup.tx_index);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::TransactionMetadata(expected_metadata));
}

#[tokio::test]
async fn markers_request() {
    let block_number = BlockNumber(0);
    let setup = setup_test_server(block_number, unique_u16!());

    let txn = setup.reader.begin_ro_txn().unwrap();

    // Test all implemented markers
    // TODO(Nadin): Add tests for GlobalRoot once it is implemented.
    let marker_tests = vec![
        (MarkerKind::State, txn.get_state_marker().unwrap()),
        (MarkerKind::Header, txn.get_header_marker().unwrap()),
        (MarkerKind::Body, txn.get_body_marker().unwrap()),
        (MarkerKind::Event, txn.get_event_marker().unwrap()),
        (MarkerKind::Class, txn.get_class_marker().unwrap()),
        (MarkerKind::CompiledClass, txn.get_compiled_class_marker().unwrap()),
        (MarkerKind::BaseLayerBlock, txn.get_base_layer_block_marker().unwrap()),
        (MarkerKind::ClassManagerBlock, txn.get_class_manager_block_marker().unwrap()),
        (
            MarkerKind::CompilerBackwardCompatibility,
            txn.get_compiler_backward_compatibility_marker().unwrap(),
        ),
    ];

    for (marker_kind, expected_marker) in marker_tests {
        let request = StorageReaderRequest::Markers(marker_kind);
        let response: StorageReaderResponse = setup.get_success_response(&request).await;

        assert_eq!(
            response,
            StorageReaderResponse::Markers(expected_marker),
            "Marker {:?} should match expected value",
            marker_kind
        );
    }
}

#[tokio::test]
async fn deployed_contracts_request() {
    let block_number = BlockNumber(0);
    let setup = setup_test_server(block_number, unique_u16!());

    let (contract_address, class_hash) = setup.state_diff.deployed_contracts.iter().next().unwrap();

    // Request the deployed contract's class hash
    let request = StorageReaderRequest::DeployedContracts(*contract_address, block_number);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::DeployedContracts(*class_hash));
}

#[tokio::test]
async fn stateless_compiled_class_hash_v2_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Test StatelessCompiledClassHashV2 request
    let request = StorageReaderRequest::StatelessCompiledClassHashV2(setup.class_hash);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(
        response,
        StorageReaderResponse::StatelessCompiledClassHashV2(setup.executable_class_hash_v2)
    );
}

#[tokio::test]
async fn transaction_hash_to_idx_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Get the transaction hash from the stored transaction
    let tx_hash = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_transaction_hash_by_idx(&setup.tx_index)
        .unwrap()
        .expect("Transaction hash should exist");

    // Test TransactionHashToIdx request
    let request = StorageReaderRequest::TransactionHashToIdx(tx_hash);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::TransactionHashToIdx(setup.tx_index));
}

#[tokio::test]
async fn last_voted_marker_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    // Test LastVotedMarker request
    let request = StorageReaderRequest::LastVotedMarker;
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::LastVotedMarker(setup.last_voted_marker));
}

#[tokio::test]
async fn file_offsets_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_offset = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_file_offset(OffsetKind::ThinStateDiff)
        .unwrap()
        .expect("File offset should exist");

    let request = StorageReaderRequest::FileOffsets(OffsetKind::ThinStateDiff);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::FileOffsets(expected_offset));
}

#[tokio::test]
async fn starknet_version_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_version = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_starknet_version_by_key(TEST_BLOCK_NUMBER)
        .unwrap()
        .expect("Starknet version should exist");

    let request = StorageReaderRequest::StarknetVersion(TEST_BLOCK_NUMBER);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::StarknetVersion(expected_version));
}

#[tokio::test]
async fn state_storage_version_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_version = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_state_version()
        .unwrap()
        .expect("State storage version should exist");

    let request = StorageReaderRequest::StateStorageVersion;
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::StateStorageVersion(expected_version));
}

#[tokio::test]
async fn blocks_storage_version_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_version = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_blocks_version()
        .unwrap()
        .expect("Blocks storage version should exist");

    let request = StorageReaderRequest::BlocksStorageVersion;
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::BlocksStorageVersion(expected_version));
}

#[tokio::test]
async fn deprecated_declared_class_block_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let request = StorageReaderRequest::DeprecatedDeclaredClassesBlock(setup.deprecated_class_hash);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::DeprecatedDeclaredClassesBlock(TEST_BLOCK_NUMBER));
}

#[tokio::test]
async fn headers_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let expected_header = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_storage_block_header(&TEST_BLOCK_NUMBER)
        .unwrap()
        .expect("Block header should exist");

    let request = StorageReaderRequest::Headers(TEST_BLOCK_NUMBER);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::Headers(expected_header));
}

#[tokio::test]
async fn block_hash_to_number_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let header = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_storage_block_header(&TEST_BLOCK_NUMBER)
        .unwrap()
        .expect("Block header should exist");

    let request = StorageReaderRequest::BlockHashToNumber(header.block_hash);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::BlockHashToNumber(TEST_BLOCK_NUMBER));
}

#[tokio::test]
async fn block_signatures_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let request = StorageReaderRequest::BlockSignatures(TEST_BLOCK_NUMBER);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::BlockSignatures(setup.block_signature));
}

#[tokio::test]
async fn events_request() {
    let setup = setup_test_server(TEST_BLOCK_NUMBER, unique_u16!());

    let tx_output = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_transaction_output(setup.tx_index)
        .unwrap()
        .expect("Transaction output should exist");
    let event_address =
        tx_output.events().first().expect("Transaction should have events").from_address;

    let request = StorageReaderRequest::Events(event_address, setup.tx_index);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::Events);
}

#[tokio::test]
async fn nonces_request() {
    let block_number = BlockNumber(0);
    let setup = setup_test_server(block_number, unique_u16!());

    // Extract the test data from the state diff
    let (contract_address, nonce) = setup.state_diff.nonces.iter().next().unwrap();
    // Request the nonce value
    let request = StorageReaderRequest::Nonces(*contract_address, block_number);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;
    assert_eq!(response, StorageReaderResponse::Nonces(*nonce));
}

#[tokio::test]
async fn compiled_class_hash_request() {
    let block_number = BlockNumber(0);
    let setup = setup_test_server(block_number, unique_u16!());

    let expected_compiled_class_hash = setup
        .reader
        .begin_ro_txn()
        .unwrap()
        .get_compiled_class_hash(setup.class_hash, block_number)
        .unwrap()
        .expect("Compiled class hash should exist");

    // Test CompiledClassHash request
    let request = StorageReaderRequest::CompiledClassHash(setup.class_hash, block_number);
    let response: StorageReaderResponse = setup.get_success_response(&request).await;

    assert_eq!(response, StorageReaderResponse::CompiledClassHash(expected_compiled_class_hash));
}
