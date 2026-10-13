use rstest::rstest;
use starknet_os::io::os_output::OsOutputError;
use starknet_proof_verifier::ProgramOutputError;

#[cfg(feature = "stwo_proving")]
use crate::errors::ProvingError;
use crate::errors::{RunnerError, VirtualSnosProverError};

#[rstest]
#[case::invalid_transaction_type(
    VirtualSnosProverError::InvalidTransactionType("Declare".to_string()),
    "invalid_transaction_type"
)]
#[case::invalid_transaction_input(
    VirtualSnosProverError::InvalidTransactionInput("tip = 1".to_string()),
    "invalid_transaction_input"
)]
#[case::validation_error(
    VirtualSnosProverError::ValidationError("Pending".to_string()),
    "validation_error"
)]
#[case::program_output_error(
    VirtualSnosProverError::ProgramOutputError(ProgramOutputError::Empty),
    "program_output_error"
)]
#[case::runner_error(
    VirtualSnosProverError::RunnerError(Box::new(RunnerError::InputGenerationError(
        "input".to_string()
    ))),
    "runner_error"
)]
#[case::output_parse_error(
    VirtualSnosProverError::OutputParseError(OsOutputError::OutputNotExhausted),
    "output_parse_error"
)]
#[case::transaction_blocked(VirtualSnosProverError::TransactionBlocked, "transaction_blocked")]
fn error_kind_names_the_variant(
    #[case] error: VirtualSnosProverError,
    #[case] expected_error_kind: &str,
) {
    assert_eq!(error.error_kind(), expected_error_kind);
}

#[cfg(feature = "stwo_proving")]
#[test]
fn error_kind_names_the_proving_variant() {
    let error =
        VirtualSnosProverError::ProvingError(ProvingError::ProverExecution("prover".to_string()));
    assert_eq!(error.error_kind(), "proving_error");
}
