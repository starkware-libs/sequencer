use std::sync::Arc;

use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::state_api::{MockStateReader, StateReader};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use mockall::predicate;
use starknet_api::class_hash;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_class_manager_types::MockClassManagerClient;

use crate::reader_with_class_manager::ReaderWithClassManager;

#[tokio::test]
async fn test_get_compiled_class() {
    let mock_state_state_reader = MockStateReader::new();
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
            Ok(ContractClass::V1((casm_contract_class.clone(), SierraVersion::default())))
        });

    let state_reader =
        ReaderWithClassManager::new(mock_state_state_reader, Arc::new(mock_class_manager_client));

    let result = state_reader.get_compiled_class(class_hash).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1((expected_result, SierraVersion::default()).try_into().unwrap())
    );
}

// TODO(noamsp): Add tests for get_storage_at, get_nonce_at, get_class_hash_at,
// get_compiled_class_hash
