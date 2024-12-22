use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures::channel::mpsc::channel;
use indexmap::IndexMap;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_storage::StorageWriter;
use papyrus_test_utils::{get_rng, get_test_block, get_test_state_diff, GetTestInstance};
use rand_chacha::rand_core::RngCore;
use starknet_api::block::{Block, BlockHeader, BlockNumber};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{SierraContractClass, StorageKey, ThinStateDiff};
use starknet_state_sync_types::errors::StateSyncError;
use starknet_types_core::felt::Felt;

use crate::StateSync;

fn get_mock_state_sync_and_storage_writer() -> (StateSync, StorageWriter) {
    let ((storage_reader, storage_writer), _) = get_test_storage();
    let state_sync = StateSync { storage_reader, new_block_sender: channel(0).0 };
    (state_sync, storage_writer)
}

#[tokio::test]
async fn test_get_block() {
    let (state_sync, mut storage_writer) = get_mock_state_sync_and_storage_writer();

    let Block { header, body } = get_test_block(1, None, None, None);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, ThinStateDiff::default())
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the block that was written is returned correctly.
    let value =
        state_sync.get_block(header.block_header_without_hash.block_number).unwrap().unwrap();
    assert_eq!(value.block_number, header.block_header_without_hash.block_number);
    assert_eq!(value.state_diff, ThinStateDiff::default());
    assert_eq!(value.transaction_hashes.len(), 1);
    assert_eq!(value.transaction_hashes[0], body.transaction_hashes[0]);

    // Verify we get Ok(None) when trying to call get_block with an unknown block number.
    let result = state_sync
        .get_block(header.block_header_without_hash.block_number.unchecked_next())
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_storage_at() {
    let (state_sync, mut storage_writer) = get_mock_state_sync_and_storage_writer();

    let mut rng = get_rng();
    let expected_address_u64 = rng.next_u64();
    let expected_address = ContractAddress::from(expected_address_u64);
    let key = StorageKey::from(rng.next_u64());
    let expected_value = Felt::from(rng.next_u64());
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.storage_diffs.insert(expected_address, IndexMap::from([(key, expected_value)]));
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the storage was written and is returned correctly.
    let value = state_sync
        .get_storage_at(header.block_header_without_hash.block_number, expected_address, key)
        .unwrap();
    assert_eq!(value, expected_value);

    // Verify we get BlockNotFound error when trying to call get_storage_at with an unknown block
    // number.
    let result = state_sync.get_storage_at(
        header.block_header_without_hash.block_number.unchecked_next(),
        expected_address,
        key,
    );
    assert_eq!(
        result,
        Err(StateSyncError::BlockNotFound(
            header.block_header_without_hash.block_number.unchecked_next()
        ))
    );

    // Verify we get BlockNotFound error when trying to call get_storage_at with an unknown
    // contract_address.
    let other_address = ContractAddress::from(expected_address_u64 + 1);
    let result = state_sync.get_storage_at(
        header.block_header_without_hash.block_number,
        other_address,
        key,
    );
    assert_eq!(result, Err(StateSyncError::ContractNotFound(other_address)));
}

#[tokio::test]
async fn test_get_nonce_at() {
    let (state_sync, mut storage_writer) = get_mock_state_sync_and_storage_writer();

    let mut rng = get_rng();
    let expected_address_u64 = rng.next_u64();
    let expected_address = ContractAddress::from(expected_address_u64);
    let expected_value = Nonce::get_test_instance(&mut rng);
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.nonces.insert(expected_address, expected_value);
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the nonce that was written is returned correctly.
    let value = state_sync
        .get_nonce_at(header.block_header_without_hash.block_number, expected_address)
        .unwrap();
    assert_eq!(value, expected_value);

    // Verify we get BlockNotFound error when trying to call get_nonce_at with an unknown block
    // number.
    let result = state_sync.get_nonce_at(
        header.block_header_without_hash.block_number.unchecked_next(),
        expected_address,
    );
    assert_eq!(
        result,
        Err(StateSyncError::BlockNotFound(
            header.block_header_without_hash.block_number.unchecked_next()
        ))
    );

    // Verify we get BlockNotFound error when trying to call get_nonce_at with an unknown
    // contract_address.
    let other_address = ContractAddress::from(expected_address_u64 + 1);
    let result =
        state_sync.get_nonce_at(header.block_header_without_hash.block_number, other_address);
    assert_eq!(result, Err(StateSyncError::ContractNotFound(other_address)));
}

#[tokio::test]
async fn get_class_hash_at() {
    let (state_sync, mut storage_writer) = get_mock_state_sync_and_storage_writer();

    let mut rng = get_rng();
    let expected_address_u64 = rng.next_u64();
    let expected_address = ContractAddress::from(expected_address_u64);
    let expected_value = ClassHash::get_test_instance(&mut rng);
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.deployed_contracts.insert(expected_address, expected_value);
    let header = BlockHeader::default();

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the class hash that was written is returned correctly.
    let result = state_sync
        .get_class_hash_at(header.block_header_without_hash.block_number, expected_address);
    let value = result.unwrap();
    assert_eq!(value, expected_value);

    // Verify we get BlockNotFound error when trying to call get_class_hash_at with an unknown block
    // number.
    let result = state_sync.get_class_hash_at(
        header.block_header_without_hash.block_number.unchecked_next(),
        expected_address,
    );
    assert_eq!(
        result,
        Err(StateSyncError::BlockNotFound(
            header.block_header_without_hash.block_number.unchecked_next()
        ))
    );

    // Verify we get BlockNotFound error when trying to call get_class_hash_at with an unknown
    // contract_address.
    let other_address = ContractAddress::from(expected_address_u64 + 1);
    let result =
        state_sync.get_class_hash_at(header.block_header_without_hash.block_number, other_address);
    assert_eq!(result, Err(StateSyncError::ContractNotFound(other_address)));
}

#[tokio::test]
async fn test_get_compiled_class_deprecated() {
    let (state_sync, mut storage_writer) = get_mock_state_sync_and_storage_writer();

    let mut rng = get_rng();
    let cairo1_class_hash = ClassHash(Felt::from(rng.next_u64()));
    let cairo1_contract_class = CasmContractClass::get_test_instance(&mut rng);
    let sierra_contract_class = SierraContractClass::default();
    let cairo0_class_hash = ClassHash(Felt::from(rng.next_u64()));
    let cairo0_contract_class = DeprecatedContractClass::get_test_instance(&mut get_rng());
    let block_number = BlockNumber(0);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(
            block_number,
            starknet_api::state::ThinStateDiff {
                declared_classes: IndexMap::from([(
                    cairo1_class_hash,
                    CompiledClassHash::default(),
                )]),
                deprecated_declared_classes: vec![cairo0_class_hash],
                ..Default::default()
            },
        )
        .unwrap()
        .append_casm(&cairo1_class_hash, &cairo1_contract_class)
        .unwrap()
        .append_classes(
            block_number,
            &[(cairo1_class_hash, &sierra_contract_class)],
            &[(cairo0_class_hash, &cairo0_contract_class)],
        )
        .unwrap()
        .commit()
        .unwrap();

    // Verify the cairo1 class was written and is returned correctly.
    let value = state_sync.get_compiled_class_deprecated(block_number, cairo1_class_hash).unwrap();
    let sierra_version =
        SierraVersion::extract_from_program(&sierra_contract_class.sierra_program).unwrap();
    assert_eq!(value, ContractClass::V1((cairo1_contract_class, sierra_version)));

    // Verify the cairo0 class was written and is returned correctly.
    let value = state_sync.get_compiled_class_deprecated(block_number, cairo0_class_hash).unwrap();
    assert_eq!(value, ContractClass::V0(cairo0_contract_class));

    // Verify we get BlockNotFound error when trying to call get_compiled_class_deprecated with an
    // unknown block number.
    let result =
        state_sync.get_compiled_class_deprecated(block_number.unchecked_next(), cairo1_class_hash);
    assert_eq!(result, Err(StateSyncError::BlockNotFound(block_number.unchecked_next())));

    // Verify we get ClassNotFound error when trying to call get_compiled_class_deprecated with an
    // unknown class hash.
    let other_class_hash = ClassHash::get_test_instance(&mut rng);
    let result = state_sync.get_compiled_class_deprecated(block_number, other_class_hash);
    assert_eq!(result, Err(StateSyncError::ClassNotFound(other_class_hash)));
}
