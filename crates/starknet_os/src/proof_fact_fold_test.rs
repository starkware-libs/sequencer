use starknet_types_core::felt::Felt;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    Blake2sDigestWords,
};
use crate::test_utils::proof_fact_fold_runner::{
    assert_cairo_combine_leaf_digests,
    assert_cairo_leaf_output_digest,
    assert_cairo_verification_digest,
};

/// The proof facts of the proving side's golden leaf: two zero version markers, followed by the
/// preimage [program_hash, ...virtual OS output] that the proving side hashes.
const GOLDEN_PROOF_FACTS: [Felt; 6] = [
    Felt::ZERO,
    Felt::ZERO,
    Felt::from_hex_unchecked("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843"),
    Felt::from_hex_unchecked("0xb"),
    Felt::from_hex_unchecked("0xd"),
    Felt::from_hex_unchecked("0x11"),
];

const GOLDEN_LEAF_OUTPUT_DIGEST: Blake2sDigestWords =
    [1603116091, 3258597502, 2711032228, 4175407283, 343882323, 1898618121, 1344732087, 1064799167];

/// The output digest of the golden leaf's processed proof, where the leaf fills both of the
/// multiverifier's verifier slots.
const GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST: Blake2sDigestWords =
    [3336922792, 2174234328, 561756268, 3019198088, 3962216814, 3753317225, 1639677005, 2558993572];

const GOLDEN_VERIFICATION_DIGEST: Blake2sDigestWords =
    [2180856259, 1333085512, 862178086, 2311453888, 551146339, 2046676941, 3386628737, 1763131494];

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    assert_eq!(compute_leaf_output_digest(&GOLDEN_PROOF_FACTS), GOLDEN_LEAF_OUTPUT_DIGEST);
    assert_cairo_leaf_output_digest(&GOLDEN_PROOF_FACTS, &GOLDEN_LEAF_OUTPUT_DIGEST);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    assert_eq!(
        combine_leaf_digests(&GOLDEN_LEAF_OUTPUT_DIGEST, &GOLDEN_LEAF_OUTPUT_DIGEST),
        GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
    );
    assert_cairo_combine_leaf_digests(
        &GOLDEN_LEAF_OUTPUT_DIGEST,
        &GOLDEN_LEAF_OUTPUT_DIGEST,
        &GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    );
    assert_eq!(
        compute_verification_digest(&GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST),
        GOLDEN_VERIFICATION_DIGEST
    );
    assert_cairo_verification_digest(
        &GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
        &GOLDEN_VERIFICATION_DIGEST,
    );
}

/// The golden fills both verifier slots with the same leaf, so it cannot catch the two leaves
/// being swapped or one being written twice.
#[test]
fn test_cairo_combine_leaf_digests_matches_rust_for_two_different_leaves() {
    let left_leaf_digest = GOLDEN_LEAF_OUTPUT_DIGEST;
    let right_leaf_digest = compute_leaf_output_digest(&[Felt::ZERO, Felt::ZERO, Felt::ONE]);
    assert_cairo_combine_leaf_digests(
        &left_leaf_digest,
        &right_leaf_digest,
        &combine_leaf_digests(&left_leaf_digest, &right_leaf_digest),
    );
}
