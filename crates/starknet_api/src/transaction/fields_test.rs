use rstest::rstest;

use super::*;

fn snos_proof_facts(proof_version: Felt) -> ProofFacts {
    ProofFacts(Arc::new(vec![
        proof_version,
        VIRTUAL_SNOS,
        Felt::from(0xABCD_u64),
        VIRTUAL_OS_OUTPUT_VERSION,
        Felt::from(0_u64),
        Felt::from(0xBEEF_u64),
        Felt::from(0xC0FFEE_u64),
    ]))
}

#[rstest]
#[case::v0(PROOF_VERSION_V0)]
#[case::v1(PROOF_VERSION_V1)]
fn proof_facts_variant_accepts_supported_versions(#[case] version: Felt) {
    let variant = ProofFactsVariant::try_from(&snos_proof_facts(version))
        .expect("supported version should parse");
    match variant {
        ProofFactsVariant::Snos(snos) => assert_eq!(snos.proof_version, version),
        ProofFactsVariant::Empty => panic!("expected Snos variant"),
    }
}

#[test]
fn proof_facts_variant_rejects_unknown_version() {
    let result = ProofFactsVariant::try_from(&snos_proof_facts(Felt::from_hex_unchecked("0xDEAD")));
    assert!(matches!(result, Err(StarknetApiError::InvalidProofFacts(_))));
}
