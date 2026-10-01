use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    Blake2sDigestWords,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};
use crate::test_utils::golden_leaf::{
    GOLDEN_LEAF_OUTPUT_DIGEST,
    GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    GOLDEN_PROOF_FACTS,
    GOLDEN_VERIFICATION_DIGEST,
};

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
