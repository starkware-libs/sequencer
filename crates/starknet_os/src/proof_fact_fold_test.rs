use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_leaf_output_digest,
    fold_block_proof_facts,
    Blake2sDigestWords,
    BLAKE2S_DIGEST_N_WORDS,
    LEAF_VERIFIER_CIRCUIT_HASH,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};

fn entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: true,
        validate_builtins_offset: true,
    }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

/// Runs a function of `proof_fact_fold.cairo` that returns a single pointer to
/// `n_returned_words` u32 words, and returns those words.
fn run_cairo_function_returning_words(
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    n_returned_words: usize,
) -> Vec<u32> {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
        MaybeRelocatable::from(Felt::ZERO);
        n_returned_words
    ]))];
    let (_, explicit_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        explicit_args,
        implicit_args,
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run Cairo function {function_name}: {error:?}"));
    let [EndpointArg::Pointer(PointerArg::Array(returned_words))] =
        explicit_return_values.as_slice()
    else {
        panic!("Expected {function_name} to return a single words-array pointer.");
    };
    returned_words
        .iter()
        .map(|returned_word| {
            let MaybeRelocatable::Int(word_felt) = returned_word else {
                panic!("Expected a felt digest word, got {returned_word:?}.");
            };
            u32::try_from(word_felt.to_biguint()).expect("A digest word must fit in a u32.")
        })
        .collect()
}

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
