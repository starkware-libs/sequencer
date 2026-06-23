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

#[test]
fn proof_facts_debug_decodes_snos_without_dumping_felts() {
    let debug = format!("{:?}", ProofFacts::snos_proof_facts_for_testing());
    // Decoded via the variant's derived `Debug`, not the raw-felt length fallback.
    assert!(debug.starts_with("ProofFacts(Snos(SnosProofFacts {"), "got: {debug}");
    assert!(debug.contains("proof_version: V1"), "got: {debug}");
    assert!(debug.contains("block_number: BlockNumber("), "got: {debug}");
    assert!(!debug.contains("elements"), "should not hit the fallback: {debug}");
}

#[test]
fn proof_facts_debug_empty() {
    assert_eq!(format!("{:?}", ProofFacts::default()), "ProofFacts(Empty)");
}

#[test]
fn proof_facts_debug_falls_back_for_unparseable() {
    let facts = ProofFacts(Arc::new(vec![Felt::from_hex_unchecked("0xDEAD")]));
    assert_eq!(format!("{:?}", facts), "ProofFacts([<1 elements>])");
}

#[test]
fn snos_proof_facts_try_from_succeeds_for_valid_snos() {
    let snos =
        SnosProofFacts::try_from(ProofFacts::snos_proof_facts_for_testing()).expect("valid SNOS");
    assert_eq!(snos.proof_version, ProofVersion::V1);
}

#[test]
fn snos_proof_facts_try_from_rejects_empty() {
    let err =
        SnosProofFacts::try_from(ProofFacts::default()).expect_err("empty should be rejected");
    let StarknetApiError::InvalidProofFacts(msg) = err else {
        panic!("expected InvalidProofFacts, got {err:?}");
    };
    assert!(msg.contains("empty"), "expected 'empty' in error, got: {msg}");
}

#[test]
fn snos_proof_facts_try_from_propagates_inner_error() {
    // A proof with an unknown version marker — the specific inner parse error should be
    // propagated directly rather than wrapped in a generic message.
    let facts = proof_facts_given_proof_version(Felt::from_hex_unchecked("0xDEAD"));
    let err = SnosProofFacts::try_from(facts).expect_err("bad version should be rejected");
    let StarknetApiError::InvalidProofFacts(msg) = err else {
        panic!("expected InvalidProofFacts, got {err:?}");
    };
    assert!(msg.contains("Expected first field"), "should propagate inner error, got: {msg}");
}
