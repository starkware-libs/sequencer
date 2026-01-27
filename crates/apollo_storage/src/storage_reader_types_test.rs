use std::net::{IpAddr, Ipv4Addr};

use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_proc_macros::unique_u16;
use apollo_test_utils::get_test_block;
use axum::http::StatusCode;
use axum::Router;
use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::{EventIndexInTransactionOutput, TransactionOffsetInBlock};
use starknet_api::{contract_address, felt, storage_key};
use tempfile::TempDir;

use crate::body::events::{EventIndex, EventsReader};
use crate::body::{BodyStorageWriter, TransactionIndex};
use crate::header::HeaderStorageWriter;
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
    from_addresses: Option<Vec<ContractAddress>>,
) -> (Router, StorageReader, ThinStateDiff, TempDir) {
    let ((reader, mut writer), temp_dir) = get_test_storage();

    let contract_address = contract_address!("0x100");
    let storage_key = storage_key!("0x10");
    let storage_value = felt!("0x42");

    let storage_diffs = IndexMap::from([(storage_key, storage_value)]);
    let state_diff = ThinStateDiff {
        storage_diffs: IndexMap::from([(contract_address, storage_diffs)]),
        ..Default::default()
    };

    let txn =
        writer.begin_rw_txn().unwrap().append_state_diff(block_number, state_diff.clone()).unwrap();

    let txn = if let Some(from_addresses) = from_addresses {
        let block = get_test_block(4, Some(3), Some(from_addresses), None);
        txn.append_header(block_number, &block.header)
            .unwrap()
            .append_body(block_number, block.body.clone())
            .unwrap()
    } else {
        txn
    };

    txn.commit().unwrap();

    let config = ServerConfig::new(
        IpAddr::from(Ipv4Addr::LOCALHOST),
        available_ports_factory(instance_index).get_next_port(),
        true,
    );
    let server = GenericStorageReaderServer::new(reader.clone(), config);
    let app = server.app();

    (app, reader, state_diff, temp_dir)
}

#[tokio::test]
async fn state_diff_location_request() {
    let block_number = BlockNumber(0);
    let (app, reader, state_diff, _temp_dir) = setup_test_server(block_number, unique_u16!(), None);

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
    let (app, _reader, state_diff, _temp_dir) =
        setup_test_server(block_number, unique_u16!(), None);

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
async fn events_request() {
    let block_number = BlockNumber(0);
    let contract_address = contract_address!("0x1");
    let from_addresses = vec![contract_address];
    let (app, reader, _state_diff, _temp_dir) =
        setup_test_server(block_number, unique_u16!(), Some(from_addresses));

    let txn = reader.begin_ro_txn().unwrap();
    let event_index = EventIndex(
        TransactionIndex(block_number, TransactionOffsetInBlock(0)),
        EventIndexInTransactionOutput(0),
    );
    let mut events_iter =
        txn.iter_events(Some(contract_address), event_index, block_number).unwrap();

    let ((found_address, EventIndex(tx_index, _)), _) =
        events_iter.next().expect("Should have at least one event from the contract address");

    assert_eq!(found_address, contract_address);

    // Verify the event exists directly
    assert!(txn.has_event(contract_address, tx_index).unwrap().is_some());

    // Test the request
    let request = StorageReaderRequest::Events(contract_address, tx_index);
    let response: StorageReaderResponse = get_response(app, &request, StatusCode::OK).await;

    assert_eq!(response, StorageReaderResponse::Events);
}
