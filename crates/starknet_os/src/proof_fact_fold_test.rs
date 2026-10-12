use std::any::Any;
use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::{
    PROOF_FACT_FOLD_BYTES,
    VERIFY_PROCESSED_PROOF_TEST_BYTES,
};
use cairo_program_runner_lib::{BootloaderHintProcessor, SIMPLE_BOOTLOADER_INPUT};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    Blake2sDigestWords,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::test_utils::cairo_runner::{
    get_all_builtins_ordered,
    initialize_and_run_cairo_0_entry_point,
    initialize_cairo_runner,
    run_cairo_0_entrypoint_with_hint_processor,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};
use crate::test_utils::golden_leaf::{
    GoldenCircuitVerifierTask,
    GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST,
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

/// The flattened single-leaf tree.
const SINGLE_LEAF_FLAT_TREE: [u8; 1] = [0];
/// The flattened four-leaf tree, in pre-order: the root fold, the fold of the first two leaves, the
/// two leaves, the fold of the last two leaves, and the two leaves.
const FOUR_LEAF_FLAT_TREE: [u8; 7] = [1, 1, 0, 0, 1, 0, 0];

/// Runs the Cairo `write_proof_tree_root_entry` on a flattened tree whose leaves are
/// `n_tx_proof_facts` copies of the golden leaf's proof facts, giving it a pointer to
/// `expected_root_output_digest`, after the multiverifier's circuit hash, to write the root entry
/// to.
fn run_cairo_proof_tree_root_entry(
    flat_tree: &[u8],
    n_tx_proof_facts: usize,
    expected_root_output_digest: &Blake2sDigestWords,
) -> Cairo0EntryPointRunnerResult<()> {
    let tx_proof_facts = (0..n_tx_proof_facts)
        .flat_map(|_| {
            [EndpointArg::from(GOLDEN_PROOF_FACTS.len()), felt_array_arg(&GOLDEN_PROOF_FACTS)]
        })
        .collect();
    let expected_root_entry: Vec<Felt> = MULTIVERIFIER_CIRCUIT_HASH
        .iter()
        .chain(expected_root_output_digest)
        .map(|word| Felt::from(*word))
        .collect();
    initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "write_proof_tree_root_entry",
        &[
            EndpointArg::from(flat_tree.len()),
            felt_array_arg(&flat_tree.iter().map(|flag| Felt::from(*flag)).collect::<Vec<_>>()),
            EndpointArg::from(n_tx_proof_facts),
            EndpointArg::Pointer(PointerArg::Composed(tx_proof_facts)),
            felt_array_arg(&expected_root_entry),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &[],
        HashMap::new(),
        None,
    )?;
    Ok(())
}

/// Runs the Cairo `verify_processed_proof` on the golden leaf's processed proof, against
/// `proof_facts`.
fn run_cairo_verify_processed_proof(proof_facts: &[Felt]) -> Cairo0EntryPointRunnerResult<()> {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let simple_bootloader_input: Box<dyn Any> =
        Box::new(verifier_task.task_input().simple_bootloader_input().unwrap());
    let implicit_args: Vec<ImplicitArg> =
        get_all_builtins_ordered()?.into_iter().map(ImplicitArg::Builtin).collect();
    let runner_config = entrypoint_runner_config();
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        VERIFY_PROCESSED_PROOF_TEST_BYTES,
        "verify_processed_proof_test",
        &implicit_args,
        HashMap::from([(SIMPLE_BOOTLOADER_INPUT.to_string(), simple_bootloader_input)]),
    )?;
    run_cairo_0_entrypoint_with_hint_processor(
        entrypoint,
        &[EndpointArg::from(proof_facts.len()), felt_array_arg(proof_facts)],
        &implicit_args,
        &mut cairo_runner,
        &program,
        &runner_config,
        &[],
        &mut BootloaderHintProcessor::new(None),
    )?;
    Ok(())
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

#[test]
fn test_cairo_verify_processed_proof_accepts_its_proof_facts() {
    run_cairo_verify_processed_proof(&GOLDEN_PROOF_FACTS).unwrap();
}

#[test]
fn test_cairo_verify_processed_proof_rejects_other_proof_facts() {
    let error = run_cairo_verify_processed_proof(&[Felt::ZERO, Felt::ZERO, Felt::ONE]).unwrap_err();
    assert!(
        error.to_string().contains("The processed proof does not verify against its proof facts."),
        "Unexpected error: {error}"
    );
}

#[rstest]
#[case::single_leaf(&SINGLE_LEAF_FLAT_TREE, 1, GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST)]
#[case::four_leaves(&FOUR_LEAF_FLAT_TREE, 4, GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST)]
fn test_cairo_proof_tree_root_matches_proving_side_golden(
    #[case] flat_tree: &[u8],
    #[case] n_tx_proof_facts: usize,
    #[case] expected_root_output_digest: Blake2sDigestWords,
) {
    run_cairo_proof_tree_root_entry(flat_tree, n_tx_proof_facts, &expected_root_output_digest)
        .unwrap();
}

/// The tree must use every transaction's proof facts, and no more.
#[rstest]
#[case::missing_proof_facts(3)]
#[case::unused_proof_facts(5)]
fn test_cairo_proof_tree_root_rejects_other_proof_facts_count(#[case] n_tx_proof_facts: usize) {
    run_cairo_proof_tree_root_entry(
        &FOUR_LEAF_FLAT_TREE,
        n_tx_proof_facts,
        &GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST,
    )
    .unwrap_err();
}
