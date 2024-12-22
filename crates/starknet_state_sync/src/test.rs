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

    let Block { header, body: expected_body } = get_test_block(1, None, None, None);
    let expected_state_diff = ThinStateDiff::from(get_test_state_diff());

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(
            header.block_header_without_hash.block_number,
            expected_state_diff.clone(),
        )
        .unwrap()
        .append_body(header.block_header_without_hash.block_number, expected_body.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Verify that the block that was written is returned correctly.
    match state_sync
        .handle_request(StateSyncRequest::GetBlock(header.block_header_without_hash.block_number))
        .await
    {
        StateSyncResponse::GetBlock(Ok(Some(block))) => {
            assert_eq!(block.block_number, header.block_header_without_hash.block_number);
            assert_eq!(block.state_diff, expected_state_diff);
            assert_eq!(block.transaction_hashes.len(), 1);
            assert_eq!(block.transaction_hashes[0], expected_body.transaction_hashes[0]);
        }
        StateSyncResponse::GetBlock(Ok(None)) => {
            panic!("No block returned");
        }
        _ => {
            panic!("Expected StateSyncResponse::GetBlock::Ok");
        }
    }
}

#[tokio::test]
async fn test_get_storage_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address_u64 = rng.next_u64();
    let address = ContractAddress::from(address_u64);
    let key = StorageKey::from(rng.next_u64());
    let expected_value = Felt::from(rng.next_u64());
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.storage_diffs.insert(address, IndexMap::from([(key, expected_value)]));
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
    match state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            header.block_header_without_hash.block_number,
            address,
            key,
        ))
        .await
    {
        StateSyncResponse::GetStorageAt(Ok(value)) => {
            assert_eq!(value, expected_value);
        }
        _ => {
            panic!("Expected StateSyncResponse::GetStorageAt::Ok");
        }
    }
}

#[tokio::test]
async fn test_get_nonce_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address_u64 = rng.next_u64();
    let address = ContractAddress::from(address_u64);
    let expected_nonce = Nonce::get_test_instance(&mut rng);
    let mut diff = ThinStateDiff::from(get_test_state_diff());
    diff.nonces.insert(address, expected_nonce);
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

    // Verify that the nonce was written and is returned correctly.
    match state_sync
        .handle_request(StateSyncRequest::GetNonceAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await
    {
        StateSyncResponse::GetNonceAt(Ok(nonce)) => {
            assert_eq!(nonce, expected_nonce);
        }
        _ => {
            panic!("Expected StateSyncResponse::GetNonceAt::Ok");
        }
    }
}

#[tokio::test]
async fn get_class_hash_at() {
    let (mut state_sync, mut storage_writer) = setup();

    let mut rng = get_rng();
    let address_u64 = rng.next_u64();
    let address = ContractAddress::from(address_u64);
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
        .commit()
        .unwrap();

    // Verify that the class hash that was written is returned correctly.
    match state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(
            header.block_header_without_hash.block_number,
            address,
        ))
        .await
    {
        StateSyncResponse::GetClassHashAt(Ok(class_hash)) => {
            assert_eq!(class_hash, expected_class_hash);
        }
        _ => {
            panic!("Expected StateSyncResponse::GetClassHashAt::Ok");
        }
    }
}

#[tokio::test]
async fn test_get_compiled_class_deprecated() {
    let (state_sync, mut storage_writer) = setup();

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
    let contract_class_v1 =
        state_sync.get_compiled_class_deprecated(block_number, cairo1_class_hash).unwrap();
    let sierra_version =
        SierraVersion::extract_from_program(&sierra_contract_class.sierra_program).unwrap();
    assert_eq!(contract_class_v1, ContractClass::V1((cairo1_contract_class, sierra_version)));

    // Verify the cairo0 class was written and is returned correctly.
    let contract_class_v0 =
        state_sync.get_compiled_class_deprecated(block_number, cairo0_class_hash).unwrap();
    assert_eq!(contract_class_v0, ContractClass::V0(cairo0_contract_class));

    let other_class_hash = ClassHash::get_test_instance(&mut rng);
    let result = state_sync.get_compiled_class_deprecated(block_number, other_class_hash);
    assert_eq!(result, Err(StateSyncError::ClassNotFound(other_class_hash)));
}

// Verify we get None/BlockNotFound when trying to call read methods with a block number that does
// not exist.
#[tokio::test]
async fn test_block_not_found() {
    let (mut state_sync, _) = setup();
    let wrong_block_number = BlockNumber(0);

    match state_sync.handle_request(StateSyncRequest::GetBlock(wrong_block_number)).await {
        StateSyncResponse::GetBlock(Ok(None)) => {}
        _ => {
            panic!("Expected StateSyncResponse::GetBlock::Ok(None)");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            wrong_block_number,
            Default::default(),
            Default::default(),
        ))
        .await
    {
        StateSyncResponse::GetStorageAt(result) => {
            assert_eq!(result, Err(StateSyncError::BlockNotFound(wrong_block_number)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetStorageAt");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetNonceAt(wrong_block_number, Default::default()))
        .await
    {
        StateSyncResponse::GetNonceAt(result) => {
            assert_eq!(result, Err(StateSyncError::BlockNotFound(wrong_block_number)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetNonceAt");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(wrong_block_number, Default::default()))
        .await
    {
        StateSyncResponse::GetClassHashAt(result) => {
            assert_eq!(result, Err(StateSyncError::BlockNotFound(wrong_block_number)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetClassHashAt");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetCompiledClassDeprecated(
            wrong_block_number,
            Default::default(),
        ))
        .await
    {
        StateSyncResponse::GetCompiledClassDeprecated(result) => {
            assert_eq!(result, Err(StateSyncError::BlockNotFound(wrong_block_number)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetCompiledClassDeprecated");
        }
    }
}

// Verify we get ContractNotFound error when trying to call get_storage_at, get_nonce_at and
// get_class_hash_at with an unknown address
#[tokio::test]
async fn test_contract_not_found() {
    let (mut state_sync, mut storage_writer) = setup();
    let address_u64 = 1_u64;
    let address = ContractAddress::from(1u64);
    let mut diff = ThinStateDiff::default();
    diff.storage_diffs.insert(address, IndexMap::from([(Default::default(), Default::default())]));
    let header = BlockHeader::default();
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(header.block_header_without_hash.block_number, &header)
        .unwrap()
        .append_state_diff(header.block_header_without_hash.block_number, diff)
        .unwrap()
        .commit()
        .unwrap();

    let non_existing_address = ContractAddress::from(address_u64 + 1);

    match state_sync
        .handle_request(StateSyncRequest::GetStorageAt(
            header.block_header_without_hash.block_number,
            non_existing_address,
            Default::default(),
        ))
        .await
    {
        StateSyncResponse::GetStorageAt(result) => {
            assert_eq!(result, Err(StateSyncError::ContractNotFound(non_existing_address)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetStorageAt");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetNonceAt(
            header.block_header_without_hash.block_number,
            non_existing_address,
        ))
        .await
    {
        StateSyncResponse::GetNonceAt(result) => {
            assert_eq!(result, Err(StateSyncError::ContractNotFound(non_existing_address)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetNonceAt");
        }
    }

    match state_sync
        .handle_request(StateSyncRequest::GetClassHashAt(
            header.block_header_without_hash.block_number,
            non_existing_address,
        ))
        .await
    {
        StateSyncResponse::GetClassHashAt(result) => {
            assert_eq!(result, Err(StateSyncError::ContractNotFound(non_existing_address)));
        }
        _ => {
            panic!("Expected StateSyncResponse::GetClassHashAt");
        }
    }
}
