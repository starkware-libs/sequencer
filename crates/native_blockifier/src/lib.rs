// The blockifier crate supports only these specific architectures.
#![cfg(any(target_pointer_width = "16", target_pointer_width = "32", target_pointer_width = "64",))]

pub mod errors;
pub mod py_block_executor;
pub mod py_declare;
pub mod py_deploy_account;
pub mod py_invoke_function;
pub mod py_l1_handler;
pub mod py_objects;
pub mod py_state_diff;
#[cfg(any(feature = "testing", test))]
pub mod py_test_utils;
// TODO(Dori, 1/4/2023): If and when supported in the Python build environment, use #[cfg(test)].
pub mod py_testing_wrappers;
pub mod py_transaction;
pub mod py_utils;
pub mod py_validator;
pub mod state_readers;
pub mod storage;
pub mod test_utils;

use blockifier::state::stateful_compression::{
    ALIAS_COUNTER_STORAGE_KEY,
    MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
    MIN_VALUE_FOR_ALIAS_ALLOC,
};
use errors::{add_py_exceptions, UndeclaredClassHashError};
use py_block_executor::PyBlockExecutor;
use py_objects::{
    PyCasmHashComputationData,
    PyCompiledClassHashesForMigration,
    PyExecutionResources,
    PyVersionedConstantsOverrides,
};
use py_validator::PyValidator;
use pyo3::prelude::*;
use starknet_api::block::StarknetVersion;
use storage::StorageConfig;

use crate::py_state_diff::PyStateDiff;
use crate::py_testing_wrappers::{
    estimate_casm_hash_computation_resources_for_testing_list,
    estimate_casm_hash_computation_resources_for_testing_single,
    raise_error_for_testing,
};

#[pymodule]
fn native_blockifier(py: Python<'_>, py_module: &PyModule) -> PyResult<()> {
    // Initialize Rust-to-Python logging.
    // Usage: just create a Python logger as usual, and it'll capture Rust prints.
    pyo3_log::init();

    py_module.add_class::<PyBlockExecutor>()?;
    py_module.add_class::<PyStateDiff>()?;
    py_module.add_class::<PyValidator>()?;
    py_module.add_class::<PyVersionedConstantsOverrides>()?;
    py_module.add_class::<PyExecutionResources>()?;
    py_module.add_class::<StorageConfig>()?;
    py_module.add_class::<PyCasmHashComputationData>()?;
    py_module.add_class::<PyCompiledClassHashesForMigration>()?;
    py_module.add("UndeclaredClassHashError", py.get_type::<UndeclaredClassHashError>())?;
    add_py_exceptions(py, py_module)?;

    py_module.add_function(wrap_pyfunction!(starknet_version, py)?)?;

    // TODO(Dori, 1/4/2023): If and when supported in the Python build environment, gate this code
    //   with #[cfg(test)].
    py_module.add_function(wrap_pyfunction!(raise_error_for_testing, py)?)?;
    py_module.add_function(wrap_pyfunction!(
        estimate_casm_hash_computation_resources_for_testing_list,
        py
    )?)?;
    py_module.add_function(wrap_pyfunction!(
        estimate_casm_hash_computation_resources_for_testing_single,
        py
    )?)?;
    py_module.add("ALIAS_COUNTER_STORAGE_KEY", ALIAS_COUNTER_STORAGE_KEY.to_string())?;
    py_module.add(
        "MAX_NON_COMPRESSED_CONTRACT_ADDRESS",
        MAX_NON_COMPRESSED_CONTRACT_ADDRESS.to_string(),
    )?;
    py_module.add("INITIAL_AVAILABLE_ALIAS", MIN_VALUE_FOR_ALIAS_ALLOC.to_string())?;

    Ok(())
}

/// Returns the latest Starknet version for versioned constants.
#[pyfunction]
pub fn starknet_version() -> PyResult<String> {
    Ok(StarknetVersion::LATEST.into())
}
