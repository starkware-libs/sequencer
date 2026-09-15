use jsonrpsee::types::ErrorObjectOwned;

use crate::errors::{PanicLocation, ProvingError, VirtualSnosProverError};

/// Returned prover errors remain internal errors.
#[test]
fn test_prover_execution_maps_to_internal_error() {
    let error =
        VirtualSnosProverError::ProvingError(ProvingError::ProverExecution("x".to_string()));

    let error_object: ErrorObjectOwned = error.into();

    assert_eq!(error_object.code(), -32603);
}

/// Capturing a panic does not change its internal-error code.
#[test]
fn test_prover_panic_maps_to_internal_error() {
    let error = VirtualSnosProverError::ProvingError(ProvingError::ProverPanic {
        location: Some(PanicLocation {
            file: "crates/stwo_prover/src/lib.rs".to_string(),
            line: 1,
        }),
        message: "out of memory".to_string(),
    });

    let error_object: ErrorObjectOwned = error.into();

    assert_eq!(error_object.code(), -32603);
}
