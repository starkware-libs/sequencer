// TODO(shahak): Test is_class_declared_at.
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::test_utils::get_test_storage;
use apollo_storage::StorageWriter;
use apollo_test_utils::{get_rng, get_test_block, get_test_state_diff, GetTestInstance};
use futures::channel::mpsc::channel;
use indexmap::IndexMap;
use rand_chacha::rand_core::RngCore;
use starknet_api::block::{Block, BlockHash, BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use starknet_state_sync_types::errors::StateSyncError;
use starknet_types_core::felt::Felt;

use crate::StateSync;

fn setup() -> (StateSync, StorageWriter) {
    let ((storage_reader, storage_writer), _) = get_test_storage();
    let state_sync = StateSync { storage_reader, new_block_sender: channel(0).0 };
    (state_sync, storage_writer)
}

#[tokio::test]
async fn test_get_block() {
    let (mut state_sync, mut storage_writer) = setup();

    let Block { header: expected_header, body: expected_body } =
        get_test_block(1, None, None, None);
    let expected_state_diff = ThinStateDiff::from(get_test_state_diff());

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(expected_header.block_header_without_hash.block_number, &expected_header)
        .unwrap()
        .append_state_diff(
            expected_header.block_header_without_hash.block_number,
            expected_state_diff.clone(),
        )
        .unwrap()
        .append_body(expected_header.block_header_without_hash.block_number, expected_body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the block was written and is returned correctly.
    let response = state_sync
        .handle_request(StateSyncRequest::GetBlock(
            expected_header.block_header_without_hash.block_number,
        ))
        .await;
    let StateSyncResponse::GetBlock(Ok(boxed_sync_block)) = response else {
        panic!("Expected StateSyncResponse::GetBlock::Ok(Box(Some(_))), but got {:?}", response);
    };
    let Some(block) = *boxed_sync_block else {
        panic!("Expected Box(Some(_)), but got {:?}", boxed_sync_block);
    };

    assert_eq!(block.block_header_without_hash, expected_header.block_header_without_hash);
    assert_eq!(block.state_diff, expected_state_diff);
    assert_eq!(block.transaction_hashes.len(), 1);
    assert_eq!(block.transaction_hashes[0], expected_body.transaction_hashes[0]);
}

#[tokio::test]
async fn test_get_storage_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address = ContractAddress::from(rng.next_u64());
    let key = StorageKey::from(rng.next_u64());
    let expected_value = Felt::from(rng.next_u64());
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.storage_diffs.insert(address, IndexMap::from([(key, expected_value)]));
    diff.deployed_contracts.insert(address, Default::default());
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the storage was written and is returned correctly.
    let response = state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            header.block_header_without_hash.block_number,
            address,
            key,
        ))
        .await;

    let StateSyncResponse::GetStorageAt(Ok(value)) = response else {
        panic!("Expected StateSyncResponse::GetStorageAt::Ok(_), but got {:?}", response);
    };

    assert_eq!(value, expected_value);
}

#[tokio::test]
async fn test_get_nonce_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address = ContractAddress::from(rng.next_u64());
    let expected_nonce = Nonce::get_test_instance(&mut rng);
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.nonces.insert(address, expected_nonce);
    diff.deployed_contracts.insert(address, Default::default());
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the nonce was written and is returned correctly.
    let response = state_sync
        .handle_request(StateSyncRequest::GetNonceAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await;

    let StateSyncResponse::GetNonceAt(Ok(nonce)) = response else {
        panic!("Expected StateSyncResponse::GetNonceAt::Ok(_), but got {:?}", response);
    };

    assert_eq!(nonce, expected_nonce);
}

#[tokio::test]
async fn get_class_hash_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address = ContractAddress::from(rng.next_u64());
    let expected_class_hash = ClassHash::get_test_instance(&mut rng);
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.deployed_contracts.insert(address, expected_class_hash);
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the class hash that was written is returned correctly.
    let response = state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await;

    let StateSyncResponse::GetClassHashAt(Ok(class_hash)) = response else {
        panic!("Expected StateSyncResponse::GetClassHashAt::Ok(_), but got {:?}", response);
    };

    assert_eq!(class_hash, expected_class_hash);
}

// Verify we get None/BlockNotFound when trying to call read methods with a block number that does
// not exist.
#[tokio::test]
async fn test_block_not_found() {
    let (mut state_sync, _) = setup();
    let non_existing_block_number = BlockNumber(0);

    let response =
        state_sync.handle_request(StateSyncRequest::GetBlock(non_existing_block_number)).await;
    let StateSyncResponse::GetBlock(Ok(maybe_block)) = response else {
        panic!("Expected StateSyncResponse::GetBlock::Ok(_), but got {:?}", response);
    };

    assert!(maybe_block.is_none());

    let response = state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            non_existing_block_number,
            Default::default(),
            Default::default(),
        ))
        .await;
    let StateSyncResponse::GetStorageAt(get_storage_at_result) = response else {
        panic!("Expected StateSyncResponse::GetStorageAt(_), but got {:?}", response);
    };

    assert_eq!(
        get_storage_at_result,
        Err(StateSyncError::BlockNotFound(non_existing_block_number))
    );

    let response = state_sync
        .handle_request(StateSyncRequest::GetNonceAt(non_existing_block_number, Default::default()))
        .await;
    let StateSyncResponse::GetNonceAt(get_nonce_at_result) = response else {
        panic!("Expected StateSyncResponse::GetNonceAt(_), but got {:?}", response);
    };

    assert_eq!(get_nonce_at_result, Err(StateSyncError::BlockNotFound(non_existing_block_number)));

    let response = state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(
            non_existing_block_number,
            Default::default(),
        ))
        .await;
    let StateSyncResponse::GetClassHashAt(get_class_hash_at_result) = response else {
        panic!("Expected StateSyncResponse::GetClassHashAt(_), but got {:?}", response);
    };

    assert_eq!(
        get_class_hash_at_result,
        Err(StateSyncError::BlockNotFound(non_existing_block_number))
    );
}

#[tokio::test]
async fn test_contract_not_found() {
    let (mut state_sync, mut storage_writer) = setup();
    let address_u64 = 2_u64;
    let address = ContractAddress::from(address_u64);
    let mut diff = ThinStateDiff::default();
    // Create corrupted state diff with a contract that was not deployed.
    diff.storage_diffs.insert(address, IndexMap::from([(Default::default(), Default::default())]));
    diff.nonces.insert(address, Default::default());
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff)
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Check that get_storage_at and get_nonce_at verify the contract was not deployed and therefore
    // return contract not found, even though the state diff is corrupt and contains this storage
    let response = state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            header.block_header_without_hash.block_number,
            address,
            Default::default(),
        ))
        .await;
    let StateSyncResponse::GetStorageAt(get_storage_at_result) = response else {
        panic!("Expected StateSyncResponse::GetStorageAt(_), but got {:?}", response);
    };

    assert_eq!(get_storage_at_result, Err(StateSyncError::ContractNotFound(address)));

    let response = state_sync
        .handle_request(StateSyncRequest::GetNonceAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await;
    let StateSyncResponse::GetNonceAt(get_nonce_at_result) = response else {
        panic!("Expected StateSyncResponse::GetNonceAt(_), but got {:?}", response);
    };

    assert_eq!(get_nonce_at_result, Err(StateSyncError::ContractNotFound(address)));

    let response = state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await;
    let StateSyncResponse::GetClassHashAt(get_class_hash_at_result) = response else {
        panic!("Expected StateSyncResponse::GetClassHashAt(_), but got {:?}", response);
    };

    assert_eq!(get_class_hash_at_result, Err(StateSyncError::ContractNotFound(address)));
}

#[tokio::test]
async fn class_declared_at_block() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let class_hash = ClassHash::get_test_instance(&mut rng);
    let compiled_class_hash = CompiledClassHash::get_test_instance(&mut rng);

    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.declared_classes.insert(class_hash, compiled_class_hash);

    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff)
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    let response = state_sync
        .handle_request(StateSyncRequest::IsClassDeclaredAt(BlockNumber(1), class_hash))
        .await;
    let StateSyncResponse::IsClassDeclaredAt(is_class_declared_at_result) = response else {
        panic!("Expected StateSyncResponse::IsClassDeclaredAt(_), but got {:?}", response);
    };

    assert_eq!(is_class_declared_at_result, Ok(true));
}

#[tokio::test]
async fn class_not_declared() {
    let (mut state_sync, _storage_writer) = setup();

    let mut rng = get_rng();
    let class_hash = ClassHash::get_test_instance(&mut rng);

    let response = state_sync
        .handle_request(StateSyncRequest::IsClassDeclaredAt(BlockNumber(0), class_hash))
        .await;
    let StateSyncResponse::IsClassDeclaredAt(is_class_declared_at_result) = response else {
        panic!("Expected StateSyncResponse::IsClassDeclaredAt(_), but got {:?}", response);
    };

    assert_eq!(is_class_declared_at_result, Ok(false));
}

#[tokio::test]
async fn class_declared_after_queried_block_number() {
    let (mut state_sync, mut storage_writer) = setup();

    // Write an empty block at block number 0
    let diff_0 = ThinStateDiff::default();
    let header_0 = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header_0.block_header_without_hash.block_number, &header_0)
        .unwrap()
        .append_state_diff(header_0.block_header_without_hash.block_number, diff_0)
        .unwrap()
        .append_body(header_0.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Write a block with a declared class at block number 1
    let class_hash = ClassHash(Felt::from(0));
    let compiled_class_hash = CompiledClassHash(StarkHash::from(Felt::from(0)));

    let mut diff_1 = ThinStateDiff::default();
    diff_1.declared_classes.insert(class_hash, compiled_class_hash);

    let header_1 = BlockHeader {
        block_hash: BlockHash(StarkHash::from(Felt::from(1))),
        block_header_without_hash: BlockHeaderWithoutHash {
            block_number: BlockNumber(1),
            ..Default::default()
        },
        ..Default::default()
    };

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header_1.block_header_without_hash.block_number, &header_1)
        .unwrap()
        .append_state_diff(header_1.block_header_without_hash.block_number, diff_1)
        .unwrap()
        .append_body(header_1.block_header_without_hash.block_number, Default::default())
        .unwrap()
        .commit()
        .unwrap();

    // Assert the class is declared by block number 1.
    let response = state_sync
        .handle_request(StateSyncRequest::IsClassDeclaredAt(BlockNumber(1), class_hash))
        .await;
    let StateSyncResponse::IsClassDeclaredAt(is_class_declared_at_result) = response else {
        panic!("Expected StateSyncResponse::IsClassDeclaredAt(_), but got {:?}", response);
    };

    assert_eq!(is_class_declared_at_result, Ok(true));

    // Assert IsClassDeclaredAt properly indicates class is not declared at block number 0
    let response = state_sync
        .handle_request(StateSyncRequest::IsClassDeclaredAt(BlockNumber(0), class_hash))
        .await;
    let StateSyncResponse::IsClassDeclaredAt(is_class_declared_at_result) = response else {
        panic!("Expected StateSyncResponse::IsClassDeclaredAt(_), but got {:?}", response);
    };

    assert_eq!(is_class_declared_at_result, Ok(false));
}
