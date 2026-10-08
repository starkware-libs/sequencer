use cairo_vm::types::builtin_name::BuiltinName;
use jsonrpsee::types::ErrorObjectOwned;

use crate::errors::{ProvingError, VirtualSnosProverError};

const UNSUPPORTED_BUILTINS_MESSAGE: &str = "Transaction uses builtins the prover does not \
                                            support: add_mod (2 instances), mul_mod (3 instances)";

fn unsupported_builtins_error() -> ProvingError {
    ProvingError::UnsupportedBuiltins {
        unsupported_builtins: vec![(BuiltinName::add_mod, 2), (BuiltinName::mul_mod, 3)],
    }
}

#[test]
fn test_unsupported_builtins_display() {
    assert_eq!(unsupported_builtins_error().to_string(), UNSUPPORTED_BUILTINS_MESSAGE);
}

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

#[test]
fn test_prover_execution_maps_to_internal_error() {
    let error_object: ErrorObjectOwned = VirtualSnosProverError::ProvingError(
        ProvingError::ProverExecution("prover failure".to_string()),
    )
    .into();

    assert_eq!(error_object.code(), -32603);
}
