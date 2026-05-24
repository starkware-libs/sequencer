use starknet_proof_verifier::ProgramOutputError;
use starknet_types_core::felt::Felt;

use super::*;

/// The revert-reason variant carries the client's transaction hash and the
/// revert string, and these failures reach the operator's log aggregator.
#[test]
fn reverted_transaction_error_is_marked_as_carrying_transaction_data() {
    let reverted = VirtualSnosProverError::RunnerError(Box::new(
        RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::TransactionReverted(
            TransactionHash(Felt::from_hex_unchecked("0x1234")),
            "insufficient balance".to_string(),
        )),
    ));

    let rendered = reverted.to_string();
    assert!(
        rendered.contains("0x1234") && rendered.contains("insufficient balance"),
        "this test assumes Display embeds the hash and revert reason, got: {rendered}"
    );
    assert!(
        reverted.may_embed_transaction_data(),
        "the revert reason and transaction hash must never be logged verbatim"
    );
}

#[test]
fn only_payload_free_variants_may_be_logged_verbatim() {
    assert!(!VirtualSnosProverError::TransactionBlocked.may_embed_transaction_data());
    assert!(
        VirtualSnosProverError::ValidationError(String::new()).may_embed_transaction_data(),
        "payload-carrying validation variants default to sensitive"
    );
}

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
