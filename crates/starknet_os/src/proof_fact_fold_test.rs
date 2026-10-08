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
    EntryPointRunnerConfig { layout: LayoutName::all_cairo, ..Default::default() }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

/// Runs the Cairo `compute_verification_digest` on two transactions' proof facts, giving it a
/// pointer to `expected_digest` to write the verification digest to. Cairo memory is write-once, so
/// the run fails unless the function writes `expected_digest`.
fn assert_cairo_verification_digest(
    left_proof_facts: &[Felt],
    right_proof_facts: &[Felt],
    expected_digest: &Blake2sDigestWords,
) {
    initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "compute_verification_digest",
        &[
            EndpointArg::from(left_proof_facts.len()),
            felt_array_arg(left_proof_facts),
            EndpointArg::from(right_proof_facts.len()),
            felt_array_arg(right_proof_facts),
            felt_array_arg(&expected_digest.map(Felt::from)),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &[],
        HashMap::new(),
        None,
    )
    .unwrap();
}

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    assert_eq!(compute_leaf_output_digest(&GOLDEN_PROOF_FACTS), GOLDEN_LEAF_OUTPUT_DIGEST);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    assert_eq!(
        combine_leaf_digests(&GOLDEN_LEAF_OUTPUT_DIGEST, &GOLDEN_LEAF_OUTPUT_DIGEST),
        GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
    );
    assert_eq!(
        compute_verification_digest(&GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST),
        GOLDEN_VERIFICATION_DIGEST
    );
}

/// The verification digest is computed from the leaf and processed proof output digests, so this
/// also checks them.
#[test]
fn test_cairo_verification_digest_matches_proving_side_golden() {
    assert_cairo_verification_digest(
        &GOLDEN_PROOF_FACTS,
        &GOLDEN_PROOF_FACTS,
        &GOLDEN_VERIFICATION_DIGEST,
    );
}
