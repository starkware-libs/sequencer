use std::collections::HashMap;

use blockifier::blockifier::transaction_executor::{BLOCK_STATE_ACCESS_ERR, DEFAULT_STACK_SIZE};
use blockifier::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use blockifier::state::state_api::StateReader;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::class_hash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::SierraContractClass;
use starknet_types_core::felt::Felt;

use crate::py_block_executor::{PyBlockExecutor, PyOsConfig};
use crate::py_objects::{PyConcurrencyConfig, PyContractClassManagerConfig};
use crate::py_state_diff::{PyBlockInfo, PyStateDiff};
use crate::py_utils::PyFelt;
use crate::test_utils::MockStorage;

const LARGE_COMPILED_CONTRACT_JSON: &str = include_str!("resources/large_compiled_contract.json");

#[test]
fn global_contract_cache_update() {
    // Initialize executor and set a contract class on the state.
    let casm = CasmContractClass {
        compiler_version: "0.1.0".to_string(),
        prime: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    };
    let sierra = SierraContractClass::default();
    let contract_class = RunnableCompiledClass::V1(
        CompiledClassV1::try_from((casm.clone(), sierra.get_sierra_version().unwrap())).unwrap(),
    );
    let class_hash = class_hash!("0x1");

    let temp_storage_path = tempfile::tempdir().unwrap().into_path();
    let mut block_executor = PyBlockExecutor::create_for_testing(
        PyConcurrencyConfig::default(),
        PyContractClassManagerConfig::default(),
        PyOsConfig::default(),
        temp_storage_path,
        4000,
        DEFAULT_STACK_SIZE,
        None,
    );
    block_executor
        .append_block(
            0,
            None,
            PyBlockInfo::default(),
            PyStateDiff::default(),
            HashMap::from([(
                class_hash.into(),
                (
                    serde_json::to_string(&sierra).unwrap(),
                    (PyFelt::from(1_u8), serde_json::to_string(&casm).unwrap()),
                ),
            )]),
            HashMap::default(),
        )
        .unwrap();

    let sentinel_block_number_and_hash = None; // Information does not exist for block 0.
    block_executor
        .setup_block_execution(
            PyBlockInfo { block_number: 1, ..PyBlockInfo::default() },
            sentinel_block_number_and_hash,
        )
        .unwrap();

    assert_eq!(block_executor.contract_class_manager.get_cache_size(), 0);

    let queried_contract_class = block_executor
        .tx_executor()
        .block_state
        .as_ref()
        .expect(BLOCK_STATE_ACCESS_ERR)
        .get_compiled_class(class_hash)
        .unwrap();

    assert_eq!(queried_contract_class, contract_class);
    assert_eq!(block_executor.contract_class_manager.get_cache_size(), 1);
}

#[test]
fn get_block_id() {
    let max_class_hash = [
        0x9, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf,
        0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf, 0xf,
    ];
    let max_class_hash_vec = Vec::from(max_class_hash);
    let expected_max_class_hash_as_py_felt = PyFelt(Felt::from_bytes_be(&max_class_hash));

    let storage =
        MockStorage { block_number_to_class_hash: HashMap::from([(1138, max_class_hash_vec)]) };
    let block_executor = PyBlockExecutor::create_for_testing_with_storage(storage);

    assert_eq!(
        block_executor.get_block_id_at_target(1138).unwrap().unwrap(),
        expected_max_class_hash_as_py_felt
    );
}

#[test]
/// Edge case: adding a large contract to the global contract cache.
fn global_contract_cache_update_large_contract() {
    let mut raw_contract_class: serde_json::Value =
        serde_json::from_str(LARGE_COMPILED_CONTRACT_JSON).unwrap();

    // ABI is not required for execution.
    raw_contract_class
        .as_object_mut()
        .expect("A compiled contract must be a JSON object.")
        .remove("abi");

    let dep_casm: DeprecatedContractClass = serde_json::from_value(raw_contract_class)
        .expect("DeprecatedContractClass is not supported for this contract.");

    let temp_storage_path = tempfile::tempdir().unwrap().into_path();
    let mut block_executor = PyBlockExecutor::native_create_for_testing(
        Default::default(),
        PyContractClassManagerConfig::default(),
        Default::default(),
        temp_storage_path,
        4000,
        DEFAULT_STACK_SIZE,
    );
    block_executor
        .append_block(
            0,
            None,
            Default::default(),
            Default::default(),
            Default::default(),
            HashMap::from([(PyFelt::from(1_u8), serde_json::to_string(&dep_casm).unwrap())]),
        )
        .unwrap();

    block_executor
        .append_block(
            1,
            Some(PyFelt(Felt::ZERO)),
            PyBlockInfo { block_number: 1, ..PyBlockInfo::default() },
            Default::default(),
            Default::default(),
            HashMap::from([(PyFelt::from(1_u8), serde_json::to_string(&dep_casm).unwrap())]),
        )
        .unwrap();
}
