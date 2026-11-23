use std::any::Any;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use apollo_starknet_os_program::test_programs::{
    PROOF_FACT_FOLD_BYTES,
    VERIFY_PROCESSED_PROOF_TEST_BYTES,
};
use cairo_program_runner_lib::{BootloaderHintProcessor, SIMPLE_BOOTLOADER_INPUT};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use leaf_proof_format::PackedNode;
use rstest::rstest;
use starknet_types_core::felt::Felt;
use tempfile::NamedTempFile;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    flatten_proof_facts_tree,
    Blake2sDigestWords,
    MULTIVERIFIER_CIRCUIT_HASH,
};
use crate::io::os_input::CircuitVerifierTaskInput;
use crate::test_utils::cairo_runner::{
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
    packed_fold,
    packed_leaf,
    GoldenCircuitVerifierTask,
    GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST,
    GOLDEN_LEAF_OUTPUT_DIGEST,
    GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    GOLDEN_PROOF_FACTS,
    GOLDEN_THREE_LEAF_PROOF_FACTS,
    GOLDEN_THREE_LEAF_ROOT_OUTPUT_DIGEST,
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

/// The `ProofFacts*` argument of the given transactions' proof facts.
fn tx_proof_facts_arg(proof_facts_list: &[&[Felt]]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Composed(
        proof_facts_list
            .iter()
            .flat_map(|proof_facts| {
                [EndpointArg::from(proof_facts.len()), felt_array_arg(proof_facts)]
            })
            .collect(),
    ))
}

/// The flattened single-leaf tree.
const SINGLE_LEAF_FLAT_TREE: [u8; 1] = [0];
/// The flattened four-leaf tree, in pre-order: the root fold, the fold of the first two leaves, the
/// two leaves, the fold of the last two leaves, and the two leaves.
const FOUR_LEAF_FLAT_TREE: [u8; 7] = [1, 1, 0, 0, 1, 0, 0];
/// The flattened three-leaf tree: the root fold, the fold of the first two leaves, the two leaves,
/// and the third leaf, carried up to the root.
const THREE_LEAF_FLAT_TREE: [u8; 5] = [1, 1, 0, 0, 0];

/// `n_tx_proof_facts` copies of the golden leaf's proof facts.
fn golden_proof_facts_copies(n_tx_proof_facts: usize) -> Vec<&'static [Felt]> {
    vec![GOLDEN_PROOF_FACTS.as_slice(); n_tx_proof_facts]
}

/// Runs the Cairo `write_proof_tree_root_entry` on a flattened tree whose leaves are the given
/// transactions' proof facts, giving it a pointer to `expected_root_output_digest`, after the
/// multiverifier's circuit hash, to write the root entry to.
fn run_cairo_proof_tree_root_entry(
    flat_tree: &[u8],
    tx_proof_facts: &[&[Felt]],
    expected_root_output_digest: &Blake2sDigestWords,
) -> Cairo0EntryPointRunnerResult<()> {
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
            EndpointArg::from(tx_proof_facts.len()),
            tx_proof_facts_arg(tx_proof_facts),
            felt_array_arg(&expected_root_entry),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &[],
        HashMap::new(),
        None,
    )?;
    Ok(())
}

/// Runs the Cairo `verify_processed_proof` on the given circuit verifier task, against the
/// single-leaf tree of `proof_facts`.
fn run_cairo_verify_processed_proof(
    verifier_task_input: &CircuitVerifierTaskInput,
    proof_facts: &[Felt],
) -> Cairo0EntryPointRunnerResult<()> {
    let simple_bootloader_input: Box<dyn Any> =
        Box::new(verifier_task_input.simple_bootloader_input().unwrap());
    let implicit_args =
        [ImplicitArg::Builtin(BuiltinName::output), ImplicitArg::Builtin(BuiltinName::range_check)];
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
        &[
            EndpointArg::from(SINGLE_LEAF_FLAT_TREE.len()),
            felt_array_arg(&SINGLE_LEAF_FLAT_TREE.map(Felt::from)),
            EndpointArg::from(1),
            tx_proof_facts_arg(&[proof_facts]),
        ],
        &implicit_args,
        &mut cairo_runner,
        &program,
        &runner_config,
        &[],
        &mut BootloaderHintProcessor::new(None),
    )?;
    Ok(())
}

/// Writes a copy of the circuit verifier executable at `executable_path` whose last bytecode word
/// differs, which makes it a different program.
fn write_other_verifier_executable(executable_path: &Path) -> NamedTempFile {
    let mut executable: serde_json::Value =
        serde_json::from_reader(File::open(executable_path).unwrap()).unwrap();
    let bytecode = executable["program"]["bytecode"].as_array_mut().unwrap();
    *bytecode.last_mut().unwrap() = serde_json::Value::String("0x1".to_string());
    let other_executable_file = NamedTempFile::new().unwrap();
    serde_json::to_writer(other_executable_file.as_file(), &executable).unwrap();
    other_executable_file
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

#[test]
fn test_cairo_verify_processed_proof_accepts_its_proof_facts() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    run_cairo_verify_processed_proof(&verifier_task.task_input(), &GOLDEN_PROOF_FACTS).unwrap();
}

#[test]
fn test_cairo_verify_processed_proof_rejects_other_proof_facts() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let mut proof_facts = GOLDEN_PROOF_FACTS;
    // Flip a single felt in the proof facts.
    proof_facts[2] += Felt::ONE;
    let error =
        run_cairo_verify_processed_proof(&verifier_task.task_input(), &proof_facts).unwrap_err();
    assert!(
        error.to_string().contains("The processed proof does not verify against its proof facts."),
        "Unexpected error: {error}"
    );
}

/// The circuit verifier's program hash is pinned: another program fails the run when the simple
/// bootloader writes its program hash, before running it.
#[test]
fn test_cairo_verify_processed_proof_rejects_other_verifier_program() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let golden_task_input = verifier_task.task_input();
    let other_executable_file = write_other_verifier_executable(&golden_task_input.executable_path);
    let other_task_input = CircuitVerifierTaskInput {
        executable_path: other_executable_file.path().to_path_buf(),
        ..golden_task_input
    };
    let error = run_cairo_verify_processed_proof(&other_task_input, &GOLDEN_PROOF_FACTS)
        .unwrap_err()
        .to_string();
    assert!(error.contains("assert [output_ptr + 1] = program_hash;"), "Unexpected error: {error}");
}

#[rstest]
#[case::single_leaf(
    &SINGLE_LEAF_FLAT_TREE,
    golden_proof_facts_copies(1),
    GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
)]
#[case::three_leaves_with_carried_leaf(
    &THREE_LEAF_FLAT_TREE,
    GOLDEN_THREE_LEAF_PROOF_FACTS.iter().map(|proof_facts| proof_facts.as_slice()).collect(),
    GOLDEN_THREE_LEAF_ROOT_OUTPUT_DIGEST
)]
#[case::four_leaves(
    &FOUR_LEAF_FLAT_TREE,
    golden_proof_facts_copies(4),
    GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST
)]
fn test_cairo_proof_tree_root_matches_proving_side_golden(
    #[case] flat_tree: &[u8],
    #[case] tx_proof_facts: Vec<&[Felt]>,
    #[case] expected_root_output_digest: Blake2sDigestWords,
) {
    run_cairo_proof_tree_root_entry(flat_tree, &tx_proof_facts, &expected_root_output_digest)
        .unwrap();
}

/// The tree must use every transaction's proof facts, and no more.
#[rstest]
#[case::missing_proof_facts(3)]
#[case::unused_proof_facts(5)]
fn test_cairo_proof_tree_root_rejects_other_proof_facts_count(#[case] n_tx_proof_facts: usize) {
    run_cairo_proof_tree_root_entry(
        &FOUR_LEAF_FLAT_TREE,
        &golden_proof_facts_copies(n_tx_proof_facts),
        &GOLDEN_FOUR_LEAF_ROOT_OUTPUT_DIGEST,
    )
    .unwrap_err();
}

/// Each packed tree flattens to the flat tree that the Cairo walk's golden tests use.
#[rstest]
#[case::single_leaf(packed_fold(vec![packed_leaf()]), &SINGLE_LEAF_FLAT_TREE)]
#[case::three_leaves_with_carried_leaf(
    packed_fold(vec![packed_fold(vec![packed_leaf(), packed_leaf()]), packed_leaf()]),
    &THREE_LEAF_FLAT_TREE
)]
#[case::four_leaves(
    packed_fold(vec![
        packed_fold(vec![packed_leaf(), packed_leaf()]),
        packed_fold(vec![packed_leaf(), packed_leaf()]),
    ]),
    &FOUR_LEAF_FLAT_TREE
)]
fn test_flatten_proof_facts_tree(
    #[case] packed_tree: PackedNode,
    #[case] expected_flat_tree: &[u8],
) {
    assert_eq!(
        flatten_proof_facts_tree(&packed_tree).unwrap(),
        expected_flat_tree.iter().map(|flag| Felt::from(*flag)).collect::<Vec<_>>()
    );
}

/// Rejects nodes that are neither a leaf nor a fold of two subtrees.
#[rstest]
#[case::preimage(PackedNode::Plain { output_preimage: vec![] })]
#[case::fold_of_three(packed_fold(vec![packed_leaf(), packed_leaf(), packed_leaf()]))]
#[case::fold_with_preimage(packed_fold(vec![packed_leaf(), PackedNode::Plain { output_preimage: vec![] }]))]
fn test_flatten_proof_facts_tree_rejects_unsupported_node(#[case] packed_tree: PackedNode) {
    flatten_proof_facts_tree(&packed_tree).unwrap_err();
}
