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
    pack_output_digest,
    Blake2sDigestWords,
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
    assert_cairo_function_writes_digest(
        "compute_leaf_output_digest",
        &[EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(proof_facts)],
        expected_digest,
    );
}

fn assert_cairo_processed_proof_output_digest(
    left_proof_facts: &[Felt],
    right_proof_facts: &[Felt],
    expected_digest: &Blake2sDigestWords,
) {
    assert_cairo_function_writes_digest(
        "compute_processed_proof_output_digest",
        &[
            EndpointArg::from(Felt::from(left_proof_facts.len())),
            felt_array_arg(left_proof_facts),
            EndpointArg::from(Felt::from(right_proof_facts.len())),
            felt_array_arg(right_proof_facts),
        ],
        expected_digest,
    );
}

fn assert_cairo_verification_digest(
    processed_proof_output_digest: &Blake2sDigestWords,
    expected_digest: &Blake2sDigestWords,
) {
    let (packed_low, packed_high) = pack_output_digest(processed_proof_output_digest);
    assert_cairo_function_writes_digest(
        "compute_verification_digest",
        &[packed_low.into(), packed_high.into()],
        expected_digest,
    );
}

/// Runs a Cairo function that writes a digest to the pointer given as its last argument, giving it
/// a pointer to `expected_digest`. Cairo memory is write-once, so the run fails unless the function
/// writes `expected_digest`.
fn assert_cairo_function_writes_digest(
    function_name: &str,
    explicit_args: &[EndpointArg],
    expected_digest: &Blake2sDigestWords,
) {
    let output_digest_arg = felt_array_arg(&expected_digest.map(Felt::from));
    initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        &[explicit_args, &[output_digest_arg]].concat(),
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
    assert_cairo_leaf_output_digest(&GOLDEN_PROOF_FACTS, &GOLDEN_LEAF_OUTPUT_DIGEST);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    assert_eq!(
        combine_leaf_digests(&GOLDEN_LEAF_OUTPUT_DIGEST, &GOLDEN_LEAF_OUTPUT_DIGEST),
        GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
    );
    assert_cairo_processed_proof_output_digest(
        &GOLDEN_PROOF_FACTS,
        &GOLDEN_PROOF_FACTS,
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
fn test_cairo_processed_proof_output_digest_matches_rust_for_two_different_leaves() {
    let right_proof_facts = [Felt::ZERO, Felt::ZERO, Felt::ONE];
    assert_cairo_processed_proof_output_digest(
        &GOLDEN_PROOF_FACTS,
        &right_proof_facts,
        &combine_leaf_digests(
            &GOLDEN_LEAF_OUTPUT_DIGEST,
            &compute_leaf_output_digest(&right_proof_facts),
        ),
    );
}
