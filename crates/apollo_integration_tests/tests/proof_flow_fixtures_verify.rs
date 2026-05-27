use apollo_integration_tests::utils::{load_proof_flow_proof, load_proof_flow_proof_facts};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use starknet_api::transaction::fields::SnosProofFacts;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_proof_verifier::verify_proof;

#[test]
fn proof_flow_fixtures_verify() {
    let proof_facts = load_proof_flow_proof_facts();
    let proof = load_proof_flow_proof();
    if let Err(err) = verify_proof(proof_facts, proof) {
        panic!(
            "Proof verification of the proof fixtures failed. The fixtures are corrupted or \
             stale. Rerun `cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features \
             starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- \
             --ignored`. Underlying error: {err}"
        );
    }
}

/// Guards against drift between the virtual-OS program hash baked into the proof-flow fixtures
/// and the `allowed_virtual_os_program_hashes` list in the latest versioned constants. If the
/// hash is no longer allowed, the proof fixtures must be regenerated.
#[test]
fn proof_flow_program_hash_is_allowed() {
    let proof_facts = load_proof_flow_proof_facts();
    let snos_proof_facts = SnosProofFacts::try_from(proof_facts)
        .expect("proof_facts.json is malformed; regenerate the fixtures");
    let allowed =
        &VersionedConstants::latest_constants().os_constants.allowed_virtual_os_program_hashes;
    assert!(
        allowed.contains(&snos_proof_facts.program_hash),
        "Virtual OS program hash {:#x} baked into the proof_flow fixtures is no longer in the \
         allowed list ({:?}). Regenerate the fixtures: `cargo +nightly-2025-07-14 test -p \
         starknet_os_flow_tests --features starknet_transaction_prover/stwo_proving --release \
         generate_proof_fixtures -- --ignored`.",
        snos_proof_facts.program_hash,
        allowed.iter().map(|h| format!("{h:#x}")).collect::<Vec<_>>(),
    );
}
