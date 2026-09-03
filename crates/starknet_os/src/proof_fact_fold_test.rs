use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use expect_test::expect;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_fold_digest,
    compute_leaf_output_digest,
    fold_block_proof_facts,
    fold_block_root_entries,
    pack_output_digest,
    unpack_output_digest,
    Blake2sDigestWords,
    FoldEntry,
    BLAKE2S_DIGEST_N_WORDS,
    LEAF_VERIFIER_CIRCUIT_HASH,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::test_utils::proof_fact_fold_runner::{
    entrypoint_runner_config,
    felt_array_arg,
    fold_entry_from_words,
    run_cairo_fold_block_proof_facts,
    run_cairo_fold_execution_resources,
    run_cairo_function_returning_words,
    FOLD_ENTRY_N_WORDS,
};

/// Proof facts shaped like a real transaction's, with values derived from
/// `transaction_index` so different transactions get different digests.
fn synthetic_proof_facts(transaction_index: u64) -> Vec<Felt> {
    vec![
        Felt::from_hex("0x50524f4f4631").unwrap(), // 'PROOF1'
        Felt::from_hex("0x5649525455414c5f534e4f53").unwrap(), // 'VIRTUAL_SNOS'
        // A program hash and block hash above 2^63, exercising the 8-word felt encoding.
        Felt::from(2u64).pow(200u64) + Felt::from(transaction_index),
        Felt::from_hex("0x5649525455414c5f534e4f5330").unwrap(), // 'VIRTUAL_SNOS0'
        Felt::from(1000 + transaction_index),
        Felt::from(2u64).pow(150u64) + Felt::from(transaction_index),
        Felt::from(transaction_index * 17),
        Felt::ONE,
        Felt::from(2u64).pow(100u64) + Felt::from(transaction_index),
    ]
}

/// Golden `H1` from proving-dev's `stwo_run_and_prove_recursive_tree` tests, with the
/// two version markers (dropped by the leaf preimage) prepended.
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

/// Golden from proving-dev's `four_leaves` fixture: four identical leaves folded over
/// two layers under the `canonical_small` registry (this module's circuit hashes).
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

#[rstest]
#[case::minimal_facts(vec![Felt::ZERO, Felt::ZERO, Felt::ONE])]
#[case::small_values(vec![Felt::ZERO, Felt::ZERO, Felt::from(42), Felt::from(1337)])]
#[case::boundary_below_2_63(vec![Felt::ZERO, Felt::ZERO, Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::ZERO, Felt::ZERO, Felt::from(1u64 << 63)])]
#[case::realistic_facts(synthetic_proof_facts(0))]
fn test_cairo_leaf_output_digest_matches_rust(#[case] proof_facts: Vec<Felt>) {
    let cairo_digest_words = run_cairo_function_returning_words(
        "compute_leaf_output_digest",
        &[EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(&proof_facts)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_digest_words, compute_leaf_output_digest(&proof_facts));
}

/// Covers the self-fold (one transaction), a full layer (two, four), the carry rule at
/// one layer (three) and at two layers (five), and a deeper tree (seven).
#[rstest]
#[case::single_transaction_self_fold(1)]
#[case::two_transactions(2)]
#[case::three_transactions_carry(3)]
#[case::four_transactions(4)]
#[case::five_transactions_double_carry(5)]
#[case::seven_transactions(7)]
fn test_cairo_fold_block_proof_facts_matches_rust(#[case] n_transactions: u64) {
    let per_transaction_proof_facts: Vec<Vec<Felt>> =
        (0..n_transactions).map(synthetic_proof_facts).collect();
    let proof_facts_references: Vec<&[Felt]> =
        per_transaction_proof_facts.iter().map(Vec::as_slice).collect();
    let cairo_root_entry = run_cairo_fold_block_proof_facts(&proof_facts_references);
    assert_eq!(cairo_root_entry, fold_block_proof_facts(&proof_facts_references));
}

#[test]
fn test_cairo_fold_digest_matches_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = fold_block_proof_facts(&[&proof_facts, &proof_facts]);
    let root_entry_words: Vec<Felt> = root_entry
        .circuit_hash
        .iter()
        .chain(root_entry.output_digest.iter())
        .map(|word| Felt::from(*word))
        .collect();
    let cairo_fold_digest_words = run_cairo_function_returning_words(
        "compute_fold_digest",
        &[felt_array_arg(&root_entry_words)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_fold_digest_words, compute_fold_digest(&root_entry));
}

#[rstest]
#[case::leaf_verifier("get_leaf_verifier_circuit_hash", LEAF_VERIFIER_CIRCUIT_HASH)]
#[case::multiverifier("get_multiverifier_circuit_hash", MULTIVERIFIER_CIRCUIT_HASH)]
fn test_cairo_circuit_hash_constants_match_rust(
    #[case] getter_name: &str,
    #[case] expected_circuit_hash: Blake2sDigestWords,
) {
    let cairo_circuit_hash_words =
        run_cairo_function_returning_words(getter_name, &[], &[], BLAKE2S_DIGEST_N_WORDS);
    assert_eq!(cairo_circuit_hash_words, expected_circuit_hash);
}

/// Block-root entries with distinct output digests, as the combine level sees them.
fn synthetic_block_root_entries(n_blocks: u64) -> Vec<FoldEntry> {
    (0..n_blocks)
        .map(|block_index| {
            let proof_facts = synthetic_proof_facts(block_index);
            fold_block_proof_facts(&[&proof_facts])
        })
        .collect()
}

/// Covers the single-entry carry (no self-fold at the combine level), a full layer, and
/// the carry rule.
#[rstest]
#[case::single_block_carried(1)]
#[case::two_blocks(2)]
#[case::three_blocks_carry(3)]
#[case::five_blocks_double_carry(5)]
fn test_cairo_fold_block_root_entries_matches_rust(#[case] n_blocks: u64) {
    let block_root_entries = synthetic_block_root_entries(n_blocks);
    let entry_words: Vec<Felt> = block_root_entries
        .iter()
        .flat_map(|entry| {
            entry
                .circuit_hash
                .iter()
                .chain(entry.output_digest.iter())
                .map(|word| Felt::from(*word))
        })
        .collect();
    let cairo_root_entry_words = run_cairo_function_returning_words(
        "fold_block_root_entries",
        &[EndpointArg::from(Felt::from(n_blocks)), felt_array_arg(&entry_words)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    assert_eq!(
        fold_entry_from_words(&cairo_root_entry_words),
        fold_block_root_entries(block_root_entries)
    );
}

#[test]
fn test_cairo_pack_and_unpack_output_digest_match_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = fold_block_proof_facts(&[&proof_facts]);
    let (expected_low, expected_high) = pack_output_digest(&root_entry.output_digest);

    // Pack in Cairo: takes the 16-word entry, returns (low, high).
    let entry_words: Vec<Felt> = root_entry
        .circuit_hash
        .iter()
        .chain(root_entry.output_digest.iter())
        .map(|word| Felt::from(*word))
        .collect();
    let expected_return_values = vec![EndpointArg::from(Felt::ZERO), EndpointArg::from(Felt::ZERO)];
    let (_, packed_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "pack_output_digest",
        &[felt_array_arg(&entry_words)],
        &[],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run pack_output_digest: {error:?}"));
    let [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_low))),
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_high))),
    ] = packed_return_values.as_slice()
    else {
        panic!("Expected pack_output_digest to return two felts.");
    };
    assert_eq!((*cairo_low, *cairo_high), (expected_low, expected_high));

    // Unpack in Cairo: takes (low, high), returns the reconstituted block-root entry.
    let unpacked_entry_words = run_cairo_function_returning_words(
        "unpack_block_root_entry",
        &[EndpointArg::from(expected_low), EndpointArg::from(expected_high)],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    assert_eq!(fold_entry_from_words(&unpacked_entry_words), root_entry);
}

#[test]
fn test_unpack_output_digest_roundtrip() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = fold_block_proof_facts(&[&proof_facts]);
    let (packed_low, packed_high) = pack_output_digest(&root_entry.output_digest);
    assert_eq!(unpack_output_digest(packed_low, packed_high), Some(root_entry.output_digest));

    // A half at or above 2^128 must be rejected.
    let felt_2_to_128 = Felt::from(2u64).pow(128u64);
    assert_eq!(unpack_output_digest(felt_2_to_128, packed_high), None);
    assert_eq!(unpack_output_digest(packed_low, felt_2_to_128 + packed_high), None);
}

/// One circuit hash of the vendored registry as digest words.
fn registry_circuit_hash(
    registry: &serde_json::Value,
    verifier_list_key: &str,
) -> Blake2sDigestWords {
    let verifier_entries = registry[verifier_list_key]
        .as_array()
        .unwrap_or_else(|| panic!("The registry must list {verifier_list_key}."));
    // The fold hardcodes a single circuit hash per role. The production registry lists
    // one leaf verifier per trace size, each with its own circuit hash; whether all
    // production leaves are proven at one canonical trace size, or the OS must allow
    // several leaf circuit hashes, must be settled before swapping the production
    // registry in - this assertion failing on such a registry forces that decision.
    assert_eq!(
        verifier_entries.len(),
        1,
        "The fold hardcodes a single circuit hash, but the registry lists {} {verifier_list_key}.",
        verifier_entries.len()
    );
    let circuit_hash_words: Vec<u32> = verifier_entries[0]["circuit_hash"]
        .as_array()
        .expect("A circuit hash must be an array of words.")
        .iter()
        .map(|circuit_hash_word| {
            let word_hex =
                circuit_hash_word.as_str().expect("A circuit hash word must be a string.");
            u32::from_str_radix(word_hex.trim_start_matches("0x"), 16)
                .expect("A circuit hash word must be a hex u32.")
        })
        .collect();
    circuit_hash_words.try_into().expect("A circuit hash must have exactly 8 words.")
}

/// Pins the circuit hash constants to the vendored circuit registry
/// (`resources/circuit_registry_canonical_small.json`): the `canonical_small` registry,
/// taken verbatim from proving-dev commit b75d21f91fe846401e002ad169dbcbe57f289ebf at
/// crates/stwo_run_and_prove_recursive_tree/test_data/circuit_registry.json. Together
/// with `test_cairo_circuit_hash_constants_match_rust` this pins the Cairo constants to
/// the registry, so swapping in the production registry is a one-file change whose
/// omissions or drift are caught here.
#[test]
fn test_circuit_hash_constants_match_vendored_registry() {
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../resources/circuit_registry_canonical_small.json"))
            .expect("The vendored circuit registry must be valid JSON.");
    assert_eq!(registry_circuit_hash(&registry, "leaf_verifiers"), LEAF_VERIFIER_CIRCUIT_HASH);
    assert_eq!(registry_circuit_hash(&registry, "multiverifiers"), MULTIVERIFIER_CIRCUIT_HASH);
}

/// Pins the Cairo fold's execution cost, answering the design's per-block budget
/// question: `fold_block_proof_facts` over `n_transactions` realistic transactions
/// (9-felt proof facts with three large felts) costs `n_transactions` leaf digests
/// (felt encoding + blake) plus the fold hashes - one self-fold for a single
/// transaction, `n_transactions - 1` pair folds otherwise. A change here means the
/// fold's cost profile changed; rerun with `UPDATE_EXPECT=1` after verifying the cause.
#[test]
fn test_fold_block_proof_facts_execution_resources() {
    let fold_resources_summary = |n_transactions: u64| {
        let per_transaction_proof_facts: Vec<Vec<Felt>> =
            (0..n_transactions).map(synthetic_proof_facts).collect();
        let proof_facts_references: Vec<&[Felt]> =
            per_transaction_proof_facts.iter().map(Vec::as_slice).collect();
        let execution_resources = run_cairo_fold_execution_resources(&proof_facts_references);
        format!(
            "{} steps, {} range checks",
            execution_resources.n_steps,
            execution_resources
                .builtin_instance_counter
                .get(&BuiltinName::range_check)
                .copied()
                .unwrap_or(0)
        )
    };
    expect!["943 steps, 13 range checks"].assert_eq(&fold_resources_summary(1));
    expect!["1281 steps, 23 range checks"].assert_eq(&fold_resources_summary(2));
    expect!["5763 steps, 101 range checks"].assert_eq(&fold_resources_summary(8));
}
