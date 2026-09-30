use starknet_os::proof_fact_fold::{combine_leaf_digests, compute_leaf_output_digest};
use starknet_os::test_utils::proof_fact_fold_runner::{
    assert_cairo_combine_leaf_digests,
    assert_cairo_leaf_output_digest,
};
use starknet_transaction_prover::verifier_task::test_utils::{
    one_leaf_proof_facts,
    run_one_leaf_verifier_task,
    FIXTURE_VERIFIER_PROGRAM_HASH,
};
use starknet_transaction_prover::verifier_task::verify_circuit_verifier_task_output;

#[test]
fn test_cairo_processed_proof_digest_matches_circuit_verifier_output() {
    let task_output = run_one_leaf_verifier_task().unwrap();
    // The single transaction's proof fills both of the multiverifier's verifier slots.
    let one_leaf_proof_facts = one_leaf_proof_facts();
    let leaf_digest = compute_leaf_output_digest(&one_leaf_proof_facts);
    assert_cairo_leaf_output_digest(&one_leaf_proof_facts, &leaf_digest);
    let output_digest = combine_leaf_digests(&leaf_digest, &leaf_digest);
    assert_cairo_combine_leaf_digests(&leaf_digest, &leaf_digest, &output_digest);
    verify_circuit_verifier_task_output(
        &task_output,
        FIXTURE_VERIFIER_PROGRAM_HASH,
        &output_digest,
    )
    .unwrap();
}
