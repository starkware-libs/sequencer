use apollo_integration_tests::state_reader::integration_test_genesis_block_hash;
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
             stale. Rerun `cargo +nightly-2026-01-15 test -p starknet_os_flow_tests --features \
             starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- \
             --ignored`. Underlying error: {err}"
        );
    }
}

/// Guards against stale proof fixtures after a genesis state change. The proof facts bake in the
/// genesis block hash, which is derived from the genesis global root — itself a function of the
/// STRK fee token address. The expect-based guards (`proof_flow_chain_info_matches_virtual_os_test`
/// and `proof_flow_global_root_matches_virtual_os_test`) can be silently refreshed with
/// `UPDATE_EXPECT=1`; this test cannot — it only passes once the proof fixtures are actually
/// regenerated against the new genesis state.
#[test]
fn proof_flow_fixtures_match_genesis_block_hash() {
    let snos_proof_facts = SnosProofFacts::try_from(load_proof_flow_proof_facts())
        .expect("proof_facts.json is malformed; regenerate the fixtures");
    let genesis_block_hash = integration_test_genesis_block_hash();
    assert_eq!(
        snos_proof_facts.block_hash, genesis_block_hash,
        "The genesis block hash baked into the proof-flow fixtures ({:#x}) does not match the \
         genesis block hash the integration test seeds into storage ({:#x}). This usually means \
         the STRK fee token address or the genesis global root changed (e.g. the expect constants \
         were refreshed with `UPDATE_EXPECT=1`) without regenerating the proof fixtures. \
         Regenerate them: `cargo +nightly-2026-01-15 test -p starknet_os_flow_tests --features \
         starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- --ignored`.",
        snos_proof_facts.block_hash.0, genesis_block_hash.0,
    );
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
         allowed list ({:?}). Regenerate the fixtures: `cargo +nightly-2026-01-15 test -p \
         starknet_os_flow_tests --features starknet_transaction_prover/stwo_proving --release \
         generate_proof_fixtures -- --ignored`.",
        snos_proof_facts.program_hash,
        allowed.iter().map(|h| format!("{h:#x}")).collect::<Vec<_>>(),
    );
}
