use std::sync::Arc;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use mockall::predicate;
use starknet_api::block::BlockNumber;
use starknet_api::class_hash;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_class_manager_types::MockClassManagerClient;

use crate::papyrus_state_wrapper::PapyrusReaderWithClassManager;

#[tokio::test]
async fn test_get_compiled_class() {
    let mut mock_class_manager_client = MockClassManagerClient::new();
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

    mock_class_manager_client
        .expect_get_executable()
        .times(1)
        .with(predicate::eq(class_hash))
        .returning(move |_| {
            Ok(ContractClass::V1((casm_contract_class.clone(), SierraVersion::default())))
        });

    // Unused in the test
    let ((storage_reader, _), _) = papyrus_storage::test_utils::get_test_storage();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(true, true);

    let state_reader = PapyrusReaderWithClassManager::new(
        storage_reader,
        block_number,
        ContractClassManager::start(contract_manager_config),
        Arc::new(mock_class_manager_client),
    );

    let result = state_reader.get_compiled_class(class_hash).unwrap();
    assert_eq!(
        result,
        RunnableCompiledClass::V1((expected_result, SierraVersion::default()).try_into().unwrap())
    );
}
