use apollo_integration_tests::utils::{load_proof_flow_proof, load_proof_flow_proof_facts};
use starknet_proof_verifier::verify_proof;

#[test]
fn proof_flow_fixtures_verify() {
    let proof_facts = load_proof_flow_proof_facts();
    let proof = load_proof_flow_proof();
    if let Err(err) = verify_proof(proof_facts, proof) {
        panic!(
            "Proof verification of the proof fixtures failed. The fixtures are corrupted or \
             stale. Rerun `./scripts/generate_proof_flow_fixtures.sh`. Underlying error: {err}"
        );
    }
}
