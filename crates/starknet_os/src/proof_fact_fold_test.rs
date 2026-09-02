use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect;
use starknet_types_core::felt::Felt;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
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

const GATED_LEAF_PROOF_TRACE_LOG_SIZE: u64 = 20;

fn entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        verify_secure: false,
        ..Default::default()
    }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

fn assert_cairo_leaf_output_digest(proof_facts: &[Felt], expected_digest: &Blake2sDigestWords) {
    assert_cairo_function_returns_digest(
        "compute_leaf_output_digest",
        &[EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(proof_facts)],
        expected_digest,
    );
}

fn assert_cairo_combine_leaf_digests(
    left_leaf_digest: &Blake2sDigestWords,
    right_leaf_digest: &Blake2sDigestWords,
    expected_digest: &Blake2sDigestWords,
) {
    assert_cairo_function_returns_digest(
        "combine_leaf_digests",
        &[
            felt_array_arg(&left_leaf_digest.map(Felt::from)),
            felt_array_arg(&right_leaf_digest.map(Felt::from)),
        ],
        expected_digest,
    );
}

fn assert_cairo_verification_digest(
    processed_proof_output_digest: &Blake2sDigestWords,
    expected_digest: &Blake2sDigestWords,
) {
    assert_cairo_function_returns_digest(
        "compute_verification_digest",
        &[felt_array_arg(&processed_proof_output_digest.map(Felt::from))],
        expected_digest,
    );
}

fn assert_cairo_function_returns_digest(
    function_name: &str,
    explicit_args: &[EndpointArg],
    expected_digest: &Blake2sDigestWords,
) {
    let expected_return_values = [felt_array_arg(&expected_digest.map(Felt::from))];
    let (_, explicit_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        explicit_args,
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(explicit_return_values, expected_return_values);
}

fn run_cairo_function_resources(
    function_name: &str,
    explicit_args: &[EndpointArg],
) -> ExecutionResources {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::from(Felt::ZERO);
            BLAKE2S_DIGEST_N_WORDS
        ]))];
    let (_, _, cairo_runner) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        explicit_args,
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run Cairo function {function_name}: {error:?}"));
    cairo_runner.get_execution_resources().unwrap().filter_unused_builtins()
}

fn format_steps_and_range_checks(execution_resources: &ExecutionResources) -> String {
    format!(
        "{} steps, {} range checks",
        execution_resources.n_steps,
        execution_resources
            .builtin_instance_counter
            .get(&BuiltinName::range_check)
            .copied()
            .unwrap_or(0)
    )
}

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
    let leaf_digest_resources = run_cairo_function_resources(
        "compute_leaf_output_digest",
        &[
            EndpointArg::from(Felt::from(GOLDEN_PROOF_FACTS.len())),
            felt_array_arg(&GOLDEN_PROOF_FACTS),
        ],
    );
    let leaf_digest = GOLDEN_LEAF_OUTPUT_DIGEST.map(Felt::from);
    // The single transaction's proof fills both of the multiverifier's verifier slots.
    let combine_resources = run_cairo_function_resources(
        "combine_leaf_digests",
        &[felt_array_arg(&leaf_digest), felt_array_arg(&leaf_digest)],
    );
    expect!["274 steps, 7 range checks"]
        .assert_eq(&format_steps_and_range_checks(&leaf_digest_resources));
    expect!["399 steps, 3 range checks"]
        .assert_eq(&format_steps_and_range_checks(&combine_resources));
}
