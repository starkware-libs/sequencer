use starknet_types_core::felt::Felt;

use super::{
    compute_leaf_output_digest,
    compute_processed_proof_output_digest,
    compute_verification_digest,
    Blake2sDigestWords,
};

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    let proving_side_preimage = [
        Felt::from_dec_str(
            "1433852663250257978909904594223798547176815246431631498282706690602142197827",
        )
        .unwrap(),
        Felt::from(11),
        Felt::from(13),
        Felt::from(17),
    ];
    let proof_facts: Vec<Felt> =
        [Felt::ZERO, Felt::ZERO].into_iter().chain(proving_side_preimage).collect();
    let expected_digest_words: Blake2sDigestWords = [
        1603116091, 3258597502, 2711032228, 4175407283, 343882323, 1898618121, 1344732087,
        1064799167,
    ];
    assert_eq!(compute_leaf_output_digest(&proof_facts), expected_digest_words);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    let proving_side_preimage = [
        Felt::from_dec_str(
            "1433852663250257978909904594223798547176815246431631498282706690602142197827",
        )
        .unwrap(),
        Felt::from(11),
        Felt::from(13),
        Felt::from(17),
    ];
    let proof_facts: Vec<Felt> =
        [Felt::ZERO, Felt::ZERO].into_iter().chain(proving_side_preimage).collect();
    let processed_proof_output_digest =
        compute_processed_proof_output_digest(&proof_facts, &proof_facts);
    let expected_output_digest: Blake2sDigestWords = [
        3336922792, 2174234328, 561756268, 3019198088, 3962216814, 3753317225, 1639677005,
        2558993572,
    ];
    assert_eq!(processed_proof_output_digest, expected_output_digest);
    let expected_verification_digest: Blake2sDigestWords = [
        2180856259, 1333085512, 862178086, 2311453888, 551146339, 2046676941, 3386628737,
        1763131494,
    ];
    assert_eq!(
        compute_verification_digest(&processed_proof_output_digest),
        expected_verification_digest
    );
}
