use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use starknet_api::proof_facts;
use starknet_api::transaction::fields::ProofFacts;
use starknet_types_core::felt::Felt;

use crate::proof_archive_writer::compute_big_storage_key;

static PROOF_FACTS_BIG_STORAGE_KEYS_FIXTURE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!())
        .join("resources/proof_facts_big_storage_keys.json")
});

/// Returns the cases pinned by the cross-language fixture.
fn big_storage_key_cases() -> Vec<ProofFacts> {
    vec![
        proof_facts![Felt::from(1u64), Felt::from(2u64), Felt::from(3u64)],
        proof_facts![Felt::ZERO],
        ProofFacts::snos_proof_facts_for_testing(),
    ]
}

/// Cross-language fixture for services that need to derive the proof archive key — regenerate with
/// UPDATE_EXPECT=1.
#[test]
fn proof_facts_big_storage_keys_fixture() {
    let fixture: Vec<serde_json::Value> = big_storage_key_cases()
        .iter()
        .map(|facts| {
            serde_json::json!({
                "proof_facts": facts.0.iter().map(|f| f.to_hex_string()).collect::<Vec<_>>(),
                "big_storage_key": compute_big_storage_key(facts),
            })
        })
        .collect();
    let json = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
    expect_file![PROOF_FACTS_BIG_STORAGE_KEYS_FIXTURE_PATH.as_path()].assert_eq(&json);
}
