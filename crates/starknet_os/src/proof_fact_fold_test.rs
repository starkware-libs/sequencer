use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_leaf_output_digest,
    compute_processed_proof_output_digest,
    compute_verification_digest,
    pack_output_digest,
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
    ValueArg,
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

fn run_cairo_processed_proof_output_digest(
    first_proof_facts: &[Felt],
    second_proof_facts: &[Felt],
) -> Vec<u32> {
    run_cairo_function_returning_words(
        "compute_processed_proof_output_digest",
        &[
            EndpointArg::from(Felt::from(first_proof_facts.len())),
            felt_array_arg(first_proof_facts),
            EndpointArg::from(Felt::from(second_proof_facts.len())),
            felt_array_arg(second_proof_facts),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    )
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
#[case::golden_shaped_facts(
    vec![Felt::ZERO, Felt::ZERO, Felt::from(11), Felt::from(13)],
    vec![Felt::ZERO, Felt::ZERO, Felt::from(11), Felt::from(13)]
)]
#[case::same_facts_twice(synthetic_proof_facts(0), synthetic_proof_facts(0))]
#[case::two_different_facts(synthetic_proof_facts(0), synthetic_proof_facts(1))]
fn test_cairo_processed_proof_output_digest_matches_rust(
    #[case] first_proof_facts: Vec<Felt>,
    #[case] second_proof_facts: Vec<Felt>,
) {
    assert_eq!(
        run_cairo_processed_proof_output_digest(&first_proof_facts, &second_proof_facts),
        compute_processed_proof_output_digest(&first_proof_facts, &second_proof_facts)
    );
}

#[test]
fn test_cairo_verification_digest_matches_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let processed_proof_output_digest =
        compute_processed_proof_output_digest(&proof_facts, &proof_facts);
    let cairo_verification_digest_words = run_cairo_function_returning_words(
        "compute_verification_digest",
        &[felt_array_arg(&processed_proof_output_digest.map(Felt::from))],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        BLAKE2S_DIGEST_N_WORDS,
    );
    assert_eq!(
        cairo_verification_digest_words,
        compute_verification_digest(&processed_proof_output_digest)
    );
}

#[test]
fn test_cairo_circuit_hashes_match_rust() {
    assert_eq!(
        run_cairo_function_returning_words(
            "get_leaf_verifier_circuit_hash",
            &[],
            &[],
            BLAKE2S_DIGEST_N_WORDS,
        ),
        LEAF_VERIFIER_CIRCUIT_HASH
    );
    assert_eq!(
        run_cairo_function_returning_words(
            "get_multiverifier_circuit_hash",
            &[],
            &[],
            BLAKE2S_DIGEST_N_WORDS,
        ),
        MULTIVERIFIER_CIRCUIT_HASH
    );
}

#[test]
fn test_cairo_pack_output_digest_matches_rust() {
    let proof_facts = synthetic_proof_facts(0);
    let output_digest = compute_processed_proof_output_digest(&proof_facts, &proof_facts);
    let (expected_low, expected_high) = pack_output_digest(&output_digest);
    let expected_return_values = vec![EndpointArg::from(Felt::ZERO), EndpointArg::from(Felt::ZERO)];
    let (_, packed_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "pack_output_digest",
        &[felt_array_arg(&output_digest.map(Felt::from))],
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
}
