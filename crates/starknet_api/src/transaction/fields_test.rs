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

// Regression test for a log-exposure bug: transaction handlers (e.g. the HTTP server's ingest
// path and the gateway's `add_tx`) log the request via `{:?}`, expecting the signature to be
// redacted. Before this fix, `TransactionSignature` derived a plain `Debug` that dumped every
// raw felt, so the signature ended up unredacted in logs.
#[test]
fn transaction_signature_debug_does_not_leak_felts() {
    let secret_looking_felt = Felt::from_hex_unchecked("0xDEADBEEF");
    let signature = TransactionSignature(Arc::new(vec![secret_looking_felt]));

    let debug = format!("{signature:?}");

    assert!(
        !debug.contains("deadbeef") && !debug.contains("DEADBEEF"),
        "signature felt leaked into Debug output: {debug}"
    );
    assert_eq!(debug, "TransactionSignature(<1 elements redacted>)");
}

#[test]
fn transaction_signature_debug_empty() {
    assert_eq!(format!("{:?}", TransactionSignature::default()), "TransactionSignature([])");
}
