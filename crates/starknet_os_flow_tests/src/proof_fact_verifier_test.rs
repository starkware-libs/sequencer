use blockifier::test_utils::get_valid_virtual_os_program_hash;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::transaction::fields::ProofFacts;
use starknet_api::{calldata, invoke_tx_args};
use starknet_os::proof_fact_fold::{compute_processed_proof_output_digest, pack_output_digest};
use starknet_os::test_utils::proof_fact_fold_runner::run_cairo_processed_proof_output_digest;
use starknet_transaction_prover::verifier_task::test_utils::{
    one_leaf_proof_facts,
    run_one_leaf_verifier_task,
    FIXTURE_VERIFIER_PROGRAM_HASH,
};
use starknet_transaction_prover::verifier_task::{
    verify_circuit_verifier_task_output,
    VerifierTaskError,
};
use starknet_types_core::felt::Felt;

use crate::test_manager::TestBuilder;

#[test]
fn test_cairo_processed_proof_digest_matches_circuit_verifier_output() {
    let task_output = run_one_leaf_verifier_task().unwrap();
    // The single transaction's proof fills both of the multiverifier's verifier slots.
    let one_leaf_proof_facts = one_leaf_proof_facts();
    let cairo_output_digest: Vec<u32> =
        run_cairo_processed_proof_output_digest(&one_leaf_proof_facts, &one_leaf_proof_facts);
    let (packed_output_low, packed_output_high) =
        pack_output_digest(&cairo_output_digest.try_into().unwrap());
    verify_circuit_verifier_task_output(
        &task_output,
        FIXTURE_VERIFIER_PROGRAM_HASH,
        packed_output_low,
        packed_output_high,
    )
    .unwrap();
}

#[tokio::test]
async fn test_os_emitted_output_feeds_verifier_task_comparison() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [test_contract_address]) =
        TestBuilder::create_standard([(test_contract, calldata![Felt::ZERO, Felt::ZERO])]).await;
    let proof_facts = ProofFacts::custom_proof_facts_for_testing(
        get_valid_virtual_os_program_hash(),
        test_builder.compute_virtual_os_config_hash(),
    );
    let calldata = create_calldata(test_contract_address, "empty_function", &[]);
    test_builder.add_funded_account_invoke(
        invoke_tx_args! { calldata: calldata.clone(), proof_facts: proof_facts.clone() },
    );

    let test_output = test_builder.build_and_run().await;
    test_output.perform_default_validations();
    let os_output = test_output
        .runner_output
        .get_os_output(test_output.private_keys.as_ref())
        .expect("Getting OsOutput from raw OS output should not fail.");
    assert_eq!(os_output.common_os_output.n_proof_facts_transactions, 1);

    let expected_output_digest =
        compute_processed_proof_output_digest(&proof_facts.0, &proof_facts.0);
    let (expected_low, expected_high) = pack_output_digest(&expected_output_digest);
    assert_eq!(os_output.common_os_output.processed_proof_output_low, expected_low);
    assert_eq!(os_output.common_os_output.processed_proof_output_high, expected_high);

    let task_output = run_one_leaf_verifier_task().unwrap();
    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            FIXTURE_VERIFIER_PROGRAM_HASH,
            os_output.common_os_output.processed_proof_output_low,
            os_output.common_os_output.processed_proof_output_high,
        ),
        Err(VerifierTaskError::VerificationDigestMismatch { .. })
    ));
}
