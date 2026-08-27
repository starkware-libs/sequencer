use starknet_types_core::felt::Felt;

use super::{
    compute_leaf_output_digest,
    fold_block_proof_facts,
    Blake2sDigestWords,
    MULTIVERIFIER_CIRCUIT_HASH,
};

/// Golden from the proving side: the leaf simple bootloader's output-preimage digest
/// (proving-dev crates/stwo_run_and_prove_recursive_tree/src/tests.rs, `H1` of the
/// preimage `[<blake program hash of simple_output_compiled.json>, 11, 13, 17]`). The
/// OS leaf preimage is proof_facts[2..], so the two version markers are prepended here.
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

/// Golden from a real proven tree on the proving side: the `four_leaves` fixture
/// (proving-dev crates/stwo_run_and_prove_recursive_tree/test_data/goldens/four_leaves),
/// four identical leaves folded over two layers under the `canonical_small` registry
/// whose circuit hashes are this module's constants. `root_outputs.json` holds the
/// expected root output digest.
#[test]
fn test_four_leaf_fold_matches_proving_side_golden() {
    let proving_side_preimage = [
        Felt::from_hex("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843")
            .unwrap(),
        Felt::from_hex("0xb").unwrap(),
        Felt::from_hex("0xd").unwrap(),
        Felt::from_hex("0x11").unwrap(),
    ];
    let proof_facts: Vec<Felt> =
        [Felt::ZERO, Felt::ZERO].into_iter().chain(proving_side_preimage).collect();
    let root_entry = fold_block_proof_facts(&[proof_facts.as_slice(); 4]);
    let expected_root_output_digest: Blake2sDigestWords = [
        897652633, 1382572116, 3969946465, 347296500, 2153515991, 2472657789, 1975506022,
        3786147232,
    ];
    assert_eq!(root_entry.output_digest, expected_root_output_digest);
    assert_eq!(root_entry.circuit_hash, MULTIVERIFIER_CIRCUIT_HASH);
}
