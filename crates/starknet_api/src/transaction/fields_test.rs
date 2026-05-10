use starknet_types_core::short_string::ShortString;
use strum::IntoEnumIterator;

use super::*;

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
