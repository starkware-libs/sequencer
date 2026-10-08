#[cfg(feature = "stwo_proving")]
use cairo_vm::types::builtin_name::BuiltinName;
use jsonrpsee::types::ErrorObjectOwned;

#[cfg(feature = "stwo_proving")]
use crate::errors::ProvingError;
use crate::errors::VirtualSnosProverError;

#[cfg(feature = "stwo_proving")]
const UNSUPPORTED_BUILTINS_MESSAGE: &str = "Transaction uses builtins the prover does not \
                                            support: add_mod (2 instances), mul_mod (3 instances)";

#[cfg(feature = "stwo_proving")]
fn unsupported_builtins_error() -> ProvingError {
    ProvingError::UnsupportedBuiltins {
        unsupported_builtins: vec![(BuiltinName::add_mod, 2), (BuiltinName::mul_mod, 3)],
    }
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_unsupported_builtins_display() {
    assert_eq!(unsupported_builtins_error().to_string(), UNSUPPORTED_BUILTINS_MESSAGE);
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_unsupported_builtins_maps_to_unsupported_builtin() {
    let error_object: ErrorObjectOwned =
        VirtualSnosProverError::ProvingError(unsupported_builtins_error()).into();

    assert_eq!(error_object.code(), 1002);
    assert_eq!(error_object.message(), "Unsupported builtin");
    let data: String =
        serde_json::from_str(error_object.data().expect("Missing error data").get()).unwrap();
    assert_eq!(data, UNSUPPORTED_BUILTINS_MESSAGE);
}

#[cfg(feature = "stwo_proving")]
#[test]
fn test_prover_execution_maps_to_internal_error() {
    let error_object: ErrorObjectOwned = VirtualSnosProverError::ProvingError(
        ProvingError::ProverExecution("prover failure".to_string()),
    )
    .into();

    assert_eq!(error_object.code(), -32603);
}

#[test]
fn blocking_check_unavailable_reaches_the_client_as_transaction_blocked() {
    assert_eq!(
        ErrorObjectOwned::from(VirtualSnosProverError::BlockingCheckUnavailable),
        ErrorObjectOwned::from(VirtualSnosProverError::TransactionBlocked),
    );
}
