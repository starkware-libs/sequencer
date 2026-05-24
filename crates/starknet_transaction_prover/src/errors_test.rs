use starknet_proof_verifier::ProgramOutputError;

use super::*;

#[test]
fn metric_outcome_maps_each_variant_to_its_label() {
    let cases = [
        (
            VirtualSnosProverError::InvalidTransactionType(String::new()),
            outcomes::FAILURE_VALIDATION,
        ),
        (
            VirtualSnosProverError::InvalidTransactionInput(String::new()),
            outcomes::FAILURE_VALIDATION,
        ),
        (VirtualSnosProverError::ValidationError(String::new()), outcomes::FAILURE_VALIDATION),
        (VirtualSnosProverError::TransactionBlocked, outcomes::FAILURE_BLOCKED),
        (
            VirtualSnosProverError::RunnerError(Box::new(RunnerError::InputGenerationError(
                String::new(),
            ))),
            outcomes::FAILURE_RUNNER,
        ),
        (
            VirtualSnosProverError::ProgramOutputError(ProgramOutputError::TooShort(0)),
            outcomes::FAILURE_OUTPUT_PARSE,
        ),
    ];

    for (error, expected) in &cases {
        assert_eq!(error.metric_outcome(), *expected, "unexpected outcome for {error:?}");
    }

    #[cfg(feature = "stwo_proving")]
    assert_eq!(
        VirtualSnosProverError::ProvingError(ProvingError::ProverExecution(String::new()))
            .metric_outcome(),
        outcomes::FAILURE_PROVING,
    );
}
