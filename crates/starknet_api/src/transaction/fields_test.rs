use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use rstest::rstest;
use starknet_types_core::short_string::ShortString;
use strum::IntoEnumIterator;

use super::*;
use crate::proof_facts;

static PROOF_FACTS_BIG_STORAGE_KEYS_FIXTURE_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!())
        .join("resources/proof_facts_big_storage_keys.json")
});

/// Returns SNOS-shaped `ProofFacts` whose first felt is the given proof version.
fn proof_facts_given_proof_version(proof_version: Felt) -> ProofFacts {
    let mut facts = ProofFacts::snos_proof_facts_for_testing();
    Arc::make_mut(&mut facts.0)[0] = proof_version;
    facts
}

#[test]
fn proof_facts_variant_accepts_supported_versions() {
    for version in ProofVersion::iter() {
        let variant =
            ProofFactsVariant::try_from(&proof_facts_given_proof_version(version.as_felt()))
                .expect("supported version should parse");
        match variant {
            ProofFactsVariant::Snos(snos) => assert_eq!(snos.proof_version, version),
            ProofFactsVariant::Empty => panic!("expected Snos variant"),
        }
    }
}

#[test]
fn proof_facts_variant_rejects_unknown_version() {
    let facts = proof_facts_given_proof_version(Felt::from_hex_unchecked("0xDEAD"));
    assert!(matches!(
        ProofFactsVariant::try_from(&facts),
        Err(StarknetApiError::InvalidProofFacts(_))
    ));
}

#[test]
fn proof_version_str_encodes_to_felt() {
    for version in ProofVersion::iter() {
        let from_short_string =
            Felt::from(ShortString::try_from(version.as_str()).expect("valid short string"));
        assert_eq!(from_short_string, version.as_felt());
    }
}

/// Returns the cases the Python recursive_prover pins against.
fn big_storage_key_cases() -> Vec<ProofFacts> {
    vec![
        proof_facts![Felt::from(1u64), Felt::from(2u64), Felt::from(3u64)],
        proof_facts![Felt::ZERO],
        ProofFacts::snos_proof_facts_for_testing(),
    ]
}

#[rstest]
#[case::small_ints(&big_storage_key_cases()[0])]
#[case::single_zero(&big_storage_key_cases()[1])]
#[case::snos(&big_storage_key_cases()[2])]
fn proof_facts_big_storage_key_starts_with_proofs(#[case] facts: &ProofFacts) {
    let key = facts.big_storage_key();
    assert!(key.starts_with("proofs/"), "unexpected prefix: {key}");
}

/// Cross-language contract: the Python recursive_prover (starkware repo) downloads this JSON
/// pinned by sequencer commit and asserts its derivation matches each entry. Regenerate via:
///     UPDATE_EXPECT=1 cargo test -p starknet_api proof_facts_big_storage_keys_fixture
#[test]
fn proof_facts_big_storage_keys_fixture() {
    let fixture: Vec<serde_json::Value> = big_storage_key_cases()
        .iter()
        .map(|facts| {
            serde_json::json!({
                "proof_facts": facts.0.iter().map(|f| f.to_hex_string()).collect::<Vec<_>>(),
                "big_storage_key": facts.big_storage_key(),
            })
        })
        .collect();
    let json = serde_json::to_string_pretty(&fixture).unwrap() + "\n";
    expect_file![PROOF_FACTS_BIG_STORAGE_KEYS_FIXTURE_PATH.as_path()].assert_eq(&json);
}
