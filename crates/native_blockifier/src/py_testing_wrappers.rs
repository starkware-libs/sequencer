use blockifier::transaction::errors::{TransactionExecutionError, TransactionFeeError};
use pyo3::pyfunction;

use crate::errors::NativeBlockifierResult;

#[pyfunction]
pub fn raise_error_for_testing() -> NativeBlockifierResult<()> {
    Err(TransactionExecutionError::TransactionFeeError(Box::new(
        TransactionFeeError::CairoResourcesNotContainedInFeeCosts,
    ))
    .into())
}
