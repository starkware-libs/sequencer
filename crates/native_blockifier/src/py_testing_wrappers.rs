use blockifier::execution::contract_class::{
    estimate_casm_poseidon_hash_computation_resources,
    FeltSizeGroups,
    NestedMultipleIntList,
};
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
    let node = NestedMultipleIntList::Leaf(
        bytecode_segment_lengths,
        FeltSizeGroups { small: 0, large: 0 },
    );
    Ok(estimate_casm_poseidon_hash_computation_resources(&node).into())
}

/// Wrapper for [estimate_casm_poseidon_hash_computation_resources] that can be used for testing.
/// Takes a node of leaves.
#[pyfunction]
pub fn estimate_casm_hash_computation_resources_for_testing_list(
    bytecode_segment_lengths: Vec<usize>,
) -> PyResult<PyExecutionResources> {
    let node = NestedMultipleIntList::Node(
        bytecode_segment_lengths
            .into_iter()
            .map(|length| {
                NestedMultipleIntList::Leaf(length, FeltSizeGroups { small: 0, large: 0 })
            })
            .collect(),
    );
    Ok(estimate_casm_poseidon_hash_computation_resources(&node).into())
}
