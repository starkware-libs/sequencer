use rstest::rstest;

use super::*;

/// Returns SNOS-shaped `ProofFacts` whose first felt is the given proof version.
fn proof_facts_given_proof_version(proof_version: Felt) -> ProofFacts {
    let mut facts = ProofFacts::snos_proof_facts_for_testing();
    Arc::make_mut(&mut facts.0)[0] = proof_version;
    facts
}

#[rstest]
#[case::v0(ProofVersion::V0)]
#[case::v1(ProofVersion::V1)]
fn proof_facts_variant_accepts_supported_versions(#[case] version: ProofVersion) {
    let variant = ProofFactsVariant::try_from(&proof_facts_given_proof_version(version.as_felt()))
        .expect("supported version should parse");
    match variant {
        ProofFactsVariant::Snos(snos) => assert_eq!(snos.proof_version, version),
        ProofFactsVariant::Empty => panic!("expected Snos variant"),
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

/// Encodes a Cairo short-string into a felt (big-endian, right-aligned).
fn short_string_to_felt(s: &str) -> Felt {
    let bytes = s.as_bytes();
    assert!(bytes.len() <= 32, "short string exceeds felt width");
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(bytes);
    Felt::from_bytes_be(&padded)
}

#[rstest]
#[case::v0(ProofVersion::V0)]
#[case::v1(ProofVersion::V1)]
fn proof_version_str_encodes_to_felt(#[case] version: ProofVersion) {
    assert_eq!(short_string_to_felt(version.as_str()), version.as_felt());
}
