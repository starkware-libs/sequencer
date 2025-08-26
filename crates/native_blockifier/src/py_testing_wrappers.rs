// TODO(AvivG): If removed, change privacy of `CasmV1HashResourceEstimate` and
// `EstimateCasmHashResources`.
use blockifier::execution::casm_hash_estimation::{
    CasmV1HashResourceEstimate,
    EstimateCasmHashResources,
};
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

/// Wrapper for [CasmV1HashResourceEstimate::estimated_resources_of_compiled_class_hash] that can be
/// used for testing. Takes a leaf.
// TODO(AvivG): Replace with V2 estimation or remove entirely as a rust test exists:
// `test_compiled_class_hash_resources_estimation`. Consider adding `single_segment` case to the
// rust test.
#[pyfunction]
pub fn estimate_casm_hash_computation_resources_for_testing_single(
    bytecode_segment_lengths: usize,
) -> PyResult<PyExecutionResources> {
    let node = NestedFeltCounts::Leaf(bytecode_segment_lengths, FeltSizeCount::default());
    Ok(CasmV1HashResourceEstimate::estimated_resources_of_compiled_class_hash(
        &node,
        &Default::default(),
    )
    .resources()
    .into())
}

/// Wrapper for [CasmV1HashResourceEstimate::estimated_resources_of_compiled_class_hash] that can be
/// used for testing. Takes a node of leaves.
// TODO(AvivG): Replace with V2 estimation or remove entirely as a rust test exists:
// `test_compiled_class_hash_resources_estimation`.
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
    Ok(CasmV1HashResourceEstimate::estimated_resources_of_compiled_class_hash(
        &node,
        &Default::default(),
    )
    .resources()
    .into())
}
