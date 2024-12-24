use std::sync::Arc;

use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::state_api::StateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use mockall::predicate;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};
use starknet_state_sync_types::communication::MockStateSyncClient;

use crate::sync_state_reader::SyncStateReader;

#[tokio::test]
async fn test_get_storage_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let storage_key = storage_key!("0x3");
    let value = felt!("0x4");
    mock_state_sync_client
        .expect_get_storage_at()
        .times(1)
        .with(
            predicate::eq(block_number),
            predicate::eq(contract_address),
            predicate::eq(storage_key),
        )
        .returning(move |_, _, _| Ok(value));

    let state_sync_reader =
        SyncStateReader::from_number(Arc::new(mock_state_sync_client), block_number);

    let result = state_sync_reader.get_storage_at(contract_address, storage_key).unwrap();
    assert_eq!(result, value);
}

#[tokio::test]
async fn test_get_nonce_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = nonce!(0x3);

    mock_state_sync_client
        .expect_get_nonce_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_sync_reader =
        SyncStateReader::from_number(Arc::new(mock_state_sync_client), block_number);

    let result = state_sync_reader.get_nonce_at(contract_address).unwrap();
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn test_get_class_hash_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = class_hash!("0x3");

    mock_state_sync_client
        .expect_get_class_hash_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_sync_reader =
        SyncStateReader::from_number(Arc::new(mock_state_sync_client), block_number);

    let result = state_sync_reader.get_class_hash_at(contract_address).unwrap();
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn test_get_compiled_class() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let block_number = BlockNumber(1);
    let class_hash = class_hash!("0x2");
    let casm_contract_class = CasmContractClass {
        compiler_version: "0.0.0".to_string(),
        prime: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    };
    let expected_result = casm_contract_class.clone();

    mock_state_sync_client
        .expect_get_compiled_class_deprecated()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .returning(move |_, _| {
            Ok(ContractClass::V1((casm_contract_class.clone(), SierraVersion::default())))
        });

    let state_sync_reader =
        SyncStateReader::from_number(Arc::new(mock_state_sync_client), block_number);

    let result = state_sync_reader.get_compiled_class(class_hash).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1((expected_result, SierraVersion::default()).try_into().unwrap())
    );
}

// TODO: Add test for get_block_info once the function is added
