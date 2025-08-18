use blockifier::execution::casm_hash_estimation::CasmV1HashResourceEstimate;
use blockifier::execution::contract_class::{FeltSizeCount, NestedFeltCounts};
use blockifier::transaction::errors::{TransactionExecutionError, TransactionFeeError};
use pyo3::{pyfunction, PyResult};

use crate::errors::NativeBlockifierResult;
use crate::py_objects::PyExecutionResources;

#[pyfunction]
pub fn raise_error_for_testing() -> NativeBlockifierResult<()> {
    Err(TransactionExecutionError::TransactionFeeError(Box::new(
        TransactionFeeError::CairoResourcesNotContainedInFeeCosts,
    ))
    .into())
}

/// Wrapper for [estimate_casm_poseidon_hash_computation_resources] that can be used for testing.
/// Takes a leaf.
#[pyfunction]
pub fn estimate_casm_hash_computation_resources_for_testing_single(
    bytecode_segment_lengths: usize,
) -> PyResult<PyExecutionResources> {
    let node = NestedFeltCounts::Leaf(bytecode_segment_lengths, FeltSizeCount::default());
    Ok(CasmV1HashResourceEstimate::estimate_casm_poseidon_hash_computation_resources(&node).into())
}

/// Wrapper for [estimate_casm_poseidon_hash_computation_resources] that can be used for testing.
/// Takes a node of leaves.
#[pyfunction]
pub fn estimate_casm_hash_computation_resources_for_testing_list(
    bytecode_segment_lengths: Vec<usize>,
) -> PyResult<PyExecutionResources> {
    let node = NestedFeltCounts::Node(
        bytecode_segment_lengths
            .into_iter()
            .map(|length| NestedFeltCounts::Leaf(length, FeltSizeCount::default()))
            .collect(),
    );
    Ok(CasmV1HashResourceEstimate::estimate_casm_poseidon_hash_computation_resources(&node).into())
}
