use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_fold_digest,
    compute_leaf_output_digest,
    single_transaction_root_entry,
    Blake2sDigestWords,
    FoldEntry,
    TransactionProofFacts,
    BLAKE2S_DIGEST_N_WORDS,
    LEAF_VERIFIER_CIRCUIT_HASHES,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};

const FOLD_ENTRY_N_WORDS: usize = 2 * BLAKE2S_DIGEST_N_WORDS;

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

fn fold_entry_from_words(entry_words: &[u32]) -> FoldEntry {
    FoldEntry {
        circuit_hash: entry_words[..BLAKE2S_DIGEST_N_WORDS].try_into().unwrap(),
        output_digest: entry_words[BLAKE2S_DIGEST_N_WORDS..].try_into().unwrap(),
    }
}

fn entry_words_as_felts(entry: &FoldEntry) -> Vec<Felt> {
    entry
        .circuit_hash
        .iter()
        .chain(entry.output_digest.iter())
        .map(|entry_word| Felt::from(*entry_word))
        .collect()
}

fn run_cairo_single_transaction_root_entry(
    transaction_proof_facts: TransactionProofFacts<'_>,
) -> FoldEntry {
    let root_entry_words = run_cairo_function_returning_words(
        "single_transaction_root_entry",
        &[
            EndpointArg::from(Felt::from(transaction_proof_facts.proof_facts.len())),
            felt_array_arg(transaction_proof_facts.proof_facts),
            EndpointArg::from(Felt::from(transaction_proof_facts.leaf_circuit_index)),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    fold_entry_from_words(&root_entry_words)
}

fn synthetic_proof_facts(transaction_index: u64) -> Vec<Felt> {
    vec![
        Felt::from_hex("0x50524f4f4631").unwrap(),
        Felt::from_hex("0x5649525455414c5f534e4f53").unwrap(),
        Felt::from(2u64).pow(200u64) + Felt::from(transaction_index),
        Felt::from_hex("0x5649525455414c5f534e4f5330").unwrap(),
        Felt::from(1000 + transaction_index),
        Felt::from(2u64).pow(150u64) + Felt::from(transaction_index),
        Felt::from(transaction_index * 17),
        Felt::ONE,
        Felt::from(2u64).pow(100u64) + Felt::from(transaction_index),
    ]
}

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
fn test_single_transaction_root_entry_matches_proving_side_golden() {
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
    let root_entry = single_transaction_root_entry(TransactionProofFacts {
        proof_facts: &proof_facts,
        leaf_circuit_index: 0,
    });
    let expected_root_output_digest: Blake2sDigestWords = [
        3336922792, 2174234328, 561756268, 3019198088, 3962216814, 3753317225, 1639677005,
        2558993572,
    ];
    assert_eq!(root_entry.circuit_hash, MULTIVERIFIER_CIRCUIT_HASH);
    assert_eq!(root_entry.output_digest, expected_root_output_digest);
    let expected_fold_digest: Blake2sDigestWords = [
        2180856259, 1333085512, 862178086, 2311453888, 551146339, 2046676941, 3386628737,
        1763131494,
    ];
    assert_eq!(compute_fold_digest(&root_entry), expected_fold_digest);
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
#[case::golden_shaped_facts(vec![Felt::ZERO, Felt::ZERO, Felt::from(11), Felt::from(13)])]
#[case::realistic_facts(synthetic_proof_facts(0))]
fn test_cairo_single_transaction_root_entry_matches_rust(#[case] proof_facts: Vec<Felt>) {
    let transaction_proof_facts =
        TransactionProofFacts { proof_facts: &proof_facts, leaf_circuit_index: 0 };
    let cairo_root_entry = run_cairo_single_transaction_root_entry(transaction_proof_facts);
    assert_eq!(cairo_root_entry, single_transaction_root_entry(transaction_proof_facts));
}

#[test]
fn test_cairo_fold_digest_matches_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let root_entry = single_transaction_root_entry(TransactionProofFacts {
        proof_facts: &proof_facts,
        leaf_circuit_index: 0,
    });
    let cairo_fold_digest_words = run_cairo_function_returning_words(
        "compute_fold_digest",
        &[felt_array_arg(&entry_words_as_felts(&root_entry))],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_fold_digest_words, compute_fold_digest(&root_entry));
}

#[test]
fn test_cairo_multiverifier_circuit_hash_matches_rust() {
    let cairo_circuit_hash_words = run_cairo_function_returning_words(
        "get_multiverifier_circuit_hash",
        &[],
        &[],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(cairo_circuit_hash_words, MULTIVERIFIER_CIRCUIT_HASH);
}

#[test]
fn test_cairo_leaf_verifier_circuit_hash_table_matches_rust() {
    for (leaf_circuit_index, expected_circuit_hash) in
        LEAF_VERIFIER_CIRCUIT_HASHES.iter().enumerate()
    {
        let cairo_circuit_hash_words = run_cairo_function_returning_words(
            "get_leaf_verifier_circuit_hash",
            &[EndpointArg::from(Felt::from(leaf_circuit_index))],
            &[ImplicitArg::Builtin(BuiltinName::range_check)],
            BLAKE2S_DIGEST_N_WORDS,
        );
        assert_eq!(&cairo_circuit_hash_words, expected_circuit_hash);
    }
}

#[test]
#[should_panic(expected = "Failed to run Cairo function get_leaf_verifier_circuit_hash")]
fn test_cairo_leaf_verifier_circuit_hash_rejects_out_of_bounds_index() {
    run_cairo_function_returning_words(
        "get_leaf_verifier_circuit_hash",
        &[EndpointArg::from(Felt::from(LEAF_VERIFIER_CIRCUIT_HASHES.len()))],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
}
