use cairo_vm::types::builtin_name::BuiltinName;
use jsonrpsee::types::ErrorObjectOwned;

use crate::errors::{ProvingError, VirtualSnosProverError};

#[test]
fn test_unsupported_builtins_display() {
    let error =
        ProvingError::UnsupportedBuiltins { unsupported_builtins: vec![(BuiltinName::mul_mod, 4)] };
    assert_eq!(
        error.to_string(),
        "Transaction uses builtins the prover does not support: mul_mod (4 instances)"
    );
}

#[test]
fn test_unsupported_builtins_maps_to_unsupported_builtin() {
    let error = VirtualSnosProverError::ProvingError(ProvingError::UnsupportedBuiltins {
        unsupported_builtins: vec![(BuiltinName::mul_mod, 4)],
    });
    let error_object: ErrorObjectOwned = error.into();
    assert_eq!(error_object.code(), 1002);
    assert_eq!(error_object.message(), "Unsupported builtin");
    let data: String = serde_json::from_str(error_object.data().unwrap().get()).unwrap();
    assert_eq!(
        data,
        "Transaction uses builtins the prover does not support: mul_mod (4 instances)"
    );
}

#[test]
fn test_prover_execution_maps_to_internal_error() {
    let error =
        VirtualSnosProverError::ProvingError(ProvingError::ProverExecution("x".to_string()));
    let error_object: ErrorObjectOwned = error.into();
    assert_eq!(error_object.code(), -32603);
}

#[tokio::test]
async fn test_prover_task_panic_maps_to_internal_error() {
    let join_error = tokio::spawn(async { panic!("unclassified panic") }).await.unwrap_err();
    let error = VirtualSnosProverError::ProvingError(ProvingError::TaskJoin(join_error));
    let error_object: ErrorObjectOwned = error.into();
    assert_eq!(error_object.code(), -32603);
}
