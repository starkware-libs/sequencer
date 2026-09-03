//! Combined tests of the OS-side proof-fact fold against a real simple-bootloader run
//! of the circuit verifier on the `four_leaves` fixture (see
//! `starknet_transaction_prover::verifier_task`).

use blockifier::test_utils::get_valid_virtual_os_program_hash;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::transaction::fields::ProofFacts;
use starknet_api::{calldata, invoke_tx_args};
use starknet_os::proof_fact_fold::pack_output_digest;
use starknet_os::test_utils::proof_fact_fold_runner::run_cairo_fold_block_proof_facts;
use starknet_transaction_prover::verifier_task::test_utils::{
    four_leaves_proof_facts,
    run_four_leaves_verifier_task,
    FIXTURE_VERIFIER_PROGRAM_HASH,
};
use starknet_transaction_prover::verifier_task::{
    verify_circuit_verifier_task_output,
    VerifierTaskError,
};
use starknet_types_core::felt::Felt;

use crate::test_manager::TestBuilder;

/// The OS's fold code against the real verifier: the Cairo0 `fold_block_proof_facts`
/// over the `four_leaves` transactions' proof facts, packed the way the OS output packs
/// the root output digest, matches the digest the circuit verifier outputs for the
/// fixture's root proof.
#[test]
fn test_cairo_fold_matches_circuit_verifier_output() {
    let task_output = run_four_leaves_verifier_task().unwrap();
    let proof_facts = four_leaves_proof_facts();
    let cairo_root_entry = run_cairo_fold_block_proof_facts(&[proof_facts.as_slice(); 4]);
    let (root_output_low, root_output_high) = pack_output_digest(&cairo_root_entry.output_digest);
    verify_circuit_verifier_task_output(
        &task_output,
        FIXTURE_VERIFIER_PROGRAM_HASH,
        root_output_low,
        root_output_high,
    )
    .unwrap();
}

/// A real OS run's emitted packed root output digest plugs into the verifier-task
/// comparison: against a verifier run over different proof facts, it is cleanly unpacked
/// and rejected as a fold-digest mismatch (not as a malformed packed digest).
///
/// The matching-digest path cannot include the OS program yet: the OS only accepts proof
/// facts whose program hash is an allowed virtual-OS hash, while the `four_leaves`
/// fixture's leaves carry the proving side's test-task hash, and the proofs the OS does
/// accept are single-leaf recursion-circuit proofs of the pinned proving stack
/// (proving-utils v0.14.3), whose circuit differs from the fold's `canonical_small`
/// registry. Until a recursive-tree fixture over OS-valid proof facts exists, the
/// matching path is covered by `test_cairo_fold_matches_circuit_verifier_output`, and
/// the OS output wiring by the fold assertions of every flow test.
#[tokio::test]
async fn test_os_emitted_fold_output_feeds_verifier_task_comparison() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [test_contract_address]) =
        TestBuilder::create_standard([(test_contract, calldata![Felt::ZERO, Felt::ZERO])]).await;
    let proof_facts = ProofFacts::custom_proof_facts_for_testing(
        get_valid_virtual_os_program_hash(),
        test_builder.compute_virtual_os_config_hash(),
    );
    let calldata = create_calldata(test_contract_address, "empty_function", &[]);
    for _ in 0..4 {
        test_builder.add_funded_account_invoke(
            invoke_tx_args! { calldata: calldata.clone(), proof_facts: proof_facts.clone() },
        );
    }

    let test_output = test_builder.build_and_run().await;
    test_output.perform_default_validations();
    let os_output = test_output
        .runner_output
        .get_os_output(test_output.private_keys.as_ref())
        .expect("Getting OsOutput from raw OS output should not fail.");
    assert_eq!(os_output.common_os_output.n_proof_facts_transactions, 4);

    let task_output = run_four_leaves_verifier_task().unwrap();
    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            FIXTURE_VERIFIER_PROGRAM_HASH,
            os_output.common_os_output.proof_facts_root_output_low,
            os_output.common_os_output.proof_facts_root_output_high,
        ),
        Err(VerifierTaskError::FoldDigestMismatch { .. })
    ));
}
