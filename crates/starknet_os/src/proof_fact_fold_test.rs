use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    compute_leaf_output_digest,
    compute_processed_proof_output_digest,
    compute_verification_digest,
    pack_output_digest,
    unpack_output_digest,
    Blake2sDigestWords,
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
    run_cairo_function_returning_words,
    run_cairo_processed_proof_output_digest,
};

fn run_cairo_processed_proof_output_digest_resources(
    first_proof_facts: &[Felt],
    second_proof_facts: &[Felt],
) -> ExecutionResources {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::from(Felt::ZERO);
            BLAKE2S_DIGEST_N_WORDS
        ]))];
    let (_, _, cairo_runner) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "compute_processed_proof_output_digest",
        &[
            EndpointArg::from(Felt::from(first_proof_facts.len())),
            felt_array_arg(first_proof_facts),
            EndpointArg::from(Felt::from(second_proof_facts.len())),
            felt_array_arg(second_proof_facts),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| {
        panic!("Failed to run compute_processed_proof_output_digest: {error:?}")
    });
    cairo_runner.get_execution_resources().unwrap().filter_unused_builtins()
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

const GATED_LEAF_PROOF_TRACE_LOG_SIZE: u64 = 20;

fn registry_circuit_hashes(
    registry: &serde_json::Value,
    verifier_list_key: &str,
) -> Vec<Blake2sDigestWords> {
    registry[verifier_list_key]
        .as_array()
        .unwrap_or_else(|| panic!("The registry must list {verifier_list_key}."))
        .iter()
        .map(|verifier_entry| {
            let circuit_hash_words: Vec<u32> = verifier_entry["circuit_hash"]
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
        })
        .collect()
}

#[test]
fn test_circuit_hash_constants_match_vendored_registry() {
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../resources/circuit_registry_canonical_small.json"))
            .expect("The vendored circuit registry must be valid JSON.");
    assert_eq!(
        registry_circuit_hashes(&registry, "leaf_verifiers"),
        vec![LEAF_VERIFIER_CIRCUIT_HASH]
    );
    assert_eq!(
        registry["leaf_verifiers"][0]["trace_log_size"].as_u64(),
        Some(GATED_LEAF_PROOF_TRACE_LOG_SIZE)
    );
    assert_eq!(
        registry_circuit_hashes(&registry, "multiverifiers"),
        vec![MULTIVERIFIER_CIRCUIT_HASH]
    );
}

#[test]
fn test_processed_proof_output_digest_execution_resources() {
    let execution_resources = run_cairo_processed_proof_output_digest_resources(
        &synthetic_proof_facts(0),
        &synthetic_proof_facts(0),
    );
    expect!["1169 steps, 23 range checks"].assert_eq(&format!(
        "{} steps, {} range checks",
        execution_resources.n_steps,
        execution_resources
            .builtin_instance_counter
            .get(&BuiltinName::range_check)
            .copied()
            .unwrap_or(0)
    ));
}

#[test]
fn test_unpack_output_digest_roundtrip() {
    let proof_facts = synthetic_proof_facts(0);
    let output_digest = compute_processed_proof_output_digest(&proof_facts, &proof_facts);
    let (packed_low, packed_high) = pack_output_digest(&output_digest);
    assert_eq!(unpack_output_digest(packed_low, packed_high), Some(output_digest));

    let felt_2_to_128 = Felt::from(2u64).pow(128u64);
    assert_eq!(unpack_output_digest(felt_2_to_128, packed_high), None);
    assert_eq!(unpack_output_digest(packed_low, felt_2_to_128 + packed_high), None);
}
