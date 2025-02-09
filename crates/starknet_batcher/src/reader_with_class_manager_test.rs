use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{MockStateReader, StateReader};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use mockall::predicate;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::{class_hash, compiled_class_hash, contract_address, felt, nonce, storage_key};
use starknet_class_manager_types::MockClassManagerClient;

use crate::reader_with_class_manager::ReaderWithClassManager;

#[tokio::test]
async fn test_inner_state_reader_happy_flow() {
    let mut mock_inner_state_reader = MockStateReader::new();
    let mock_class_manager_client = MockClassManagerClient::new();

    let contract_address = contract_address!("0x2");
    let storage_key = storage_key!("0x3");
    let class_hash = class_hash!("0x4");

    let expected_get_storage_at_result = felt!("0x5");
    let expected_get_nonce_at_result = nonce!(0x6);
    let expected_get_class_hash_at_result = class_hash!("0x7");
    let expected_get_compiled_class_hash_result = compiled_class_hash!(0x8);

    mock_inner_state_reader
        .expect_get_storage_at()
        .times(1)
        .with(predicate::eq(contract_address), predicate::eq(storage_key))
        .returning(move |_, _| Ok(expected_get_storage_at_result));
    mock_inner_state_reader
        .expect_get_nonce_at()
        .times(1)
        .with(predicate::eq(contract_address))
        .returning(move |_| Ok(expected_get_nonce_at_result));
    mock_inner_state_reader
        .expect_get_class_hash_at()
        .times(1)
        .with(predicate::eq(contract_address))
        .returning(move |_| Ok(expected_get_class_hash_at_result));
    mock_inner_state_reader
        .expect_get_compiled_class_hash()
        .times(1)
        .with(predicate::eq(class_hash))
        .returning(move |_| Ok(expected_get_compiled_class_hash_result));

    let state_reader =
        ReaderWithClassManager::new(mock_inner_state_reader, Arc::new(mock_class_manager_client));

    let result = state_reader.get_storage_at(contract_address, storage_key).unwrap();
    assert_eq!(result, expected_get_storage_at_result);
    let result = state_reader.get_nonce_at(contract_address).unwrap();
    assert_eq!(result, expected_get_nonce_at_result);
    let result = state_reader.get_class_hash_at(contract_address).unwrap();
    assert_eq!(result, expected_get_class_hash_at_result);
    let result = state_reader.get_compiled_class_hash(class_hash).unwrap();
    assert_eq!(result, expected_get_compiled_class_hash_result);
}

#[tokio::test]
async fn test_inner_state_reader_negative_flow() {
    let mut mock_inner_state_reader = MockStateReader::new();
    let mock_class_manager_client = MockClassManagerClient::new();

    let contract_address = contract_address!("0x2");
    let storage_key = storage_key!("0x3");
    let class_hash = class_hash!("0x4");

    let expected_err_result_str = "Testing inner state reader error propogation";
    mock_inner_state_reader
        .expect_get_storage_at()
        .times(1)
        .with(predicate::eq(contract_address), predicate::eq(storage_key))
        .returning(move |_, _| {
            Err(StateError::StateReadError(expected_err_result_str.to_string()))
        });
    mock_inner_state_reader
        .expect_get_nonce_at()
        .times(1)
        .with(predicate::eq(contract_address))
        .returning(move |_| Err(StateError::StateReadError(expected_err_result_str.to_string())));
    mock_inner_state_reader
        .expect_get_class_hash_at()
        .times(1)
        .with(predicate::eq(contract_address))
        .returning(move |_| Err(StateError::StateReadError(expected_err_result_str.to_string())));
    mock_inner_state_reader
        .expect_get_compiled_class_hash()
        .times(1)
        .with(predicate::eq(class_hash))
        .returning(move |_| Err(StateError::StateReadError(expected_err_result_str.to_string())));

    let state_reader =
        ReaderWithClassManager::new(mock_inner_state_reader, Arc::new(mock_class_manager_client));

    let expect_err_str = "Expected Err(_), received Ok(_)";
    let result =
        state_reader.get_storage_at(contract_address, storage_key).expect_err(expect_err_str);
    assert_matches!(result, StateError::StateReadError(_));
    let result = state_reader.get_nonce_at(contract_address).expect_err(expect_err_str);
    assert_matches!(result, StateError::StateReadError(_));
    let result = state_reader.get_class_hash_at(contract_address).expect_err(expect_err_str);
    assert_matches!(result, StateError::StateReadError(_));
    let result = state_reader.get_compiled_class_hash(class_hash).expect_err(expect_err_str);
    assert_matches!(result, StateError::StateReadError(_));
}

// TODO(NoamS): test undeclared class flow (when class manager client returns None).
#[tokio::test]
async fn test_get_compiled_class() {
    let mock_inner_state_reader = MockStateReader::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();
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

    mock_class_manager_client
        .expect_get_executable()
        .times(1)
        .with(predicate::eq(class_hash))
        .returning(move |_| {
            Ok(Some(ContractClass::V1((casm_contract_class.clone(), SierraVersion::default()))))
        });

    let state_reader =
        ReaderWithClassManager::new(mock_inner_state_reader, Arc::new(mock_class_manager_client));

    let result = state_reader.get_compiled_class(class_hash).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1((expected_result, SierraVersion::default()).try_into().unwrap())
    );
}
