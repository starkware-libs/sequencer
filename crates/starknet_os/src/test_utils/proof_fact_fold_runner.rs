//! Runs functions of the OS's `proof_fact_fold.cairo` as standalone Cairo0 entry points,
//! for tests comparing them against the Rust mirror and against the circuit verifier.

use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_types_core::felt::Felt;

use crate::proof_fact_fold::{FoldEntry, BLAKE2S_DIGEST_N_WORDS};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};

pub const FOLD_ENTRY_N_WORDS: usize = 2 * BLAKE2S_DIGEST_N_WORDS;

pub fn entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: true,
        validate_builtins_offset: true,
    }
}

pub fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

/// Runs a function of `proof_fact_fold.cairo` that returns a single pointer to
/// `n_returned_words` u32 words, and returns those words.
pub fn run_cairo_function_returning_words(
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

pub fn fold_entry_from_words(entry_words: &[u32]) -> FoldEntry {
    FoldEntry {
        circuit_hash: entry_words[..BLAKE2S_DIGEST_N_WORDS].try_into().unwrap(),
        output_digest: entry_words[BLAKE2S_DIGEST_N_WORDS..].try_into().unwrap(),
    }
}

/// Runs the Cairo `fold_block_proof_facts` and returns the root entry.
pub fn run_cairo_fold_block_proof_facts(per_transaction_proof_facts: &[&[Felt]]) -> FoldEntry {
    let root_entry_words = run_cairo_function_returning_words(
        "fold_block_proof_facts",
        &fold_block_proof_facts_args(per_transaction_proof_facts),
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        FOLD_ENTRY_N_WORDS,
    );
    fold_entry_from_words(&root_entry_words)
}

/// Runs the Cairo `fold_block_proof_facts` and returns the run's execution resources.
pub fn run_cairo_fold_execution_resources(
    per_transaction_proof_facts: &[&[Felt]],
) -> ExecutionResources {
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Array(vec![
            MaybeRelocatable::from(Felt::ZERO);
            FOLD_ENTRY_N_WORDS
        ]))];
    let (_, _, cairo_runner) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "fold_block_proof_facts",
        &fold_block_proof_facts_args(per_transaction_proof_facts),
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap_or_else(|error| panic!("Failed to run fold_block_proof_facts: {error:?}"));
    cairo_runner.get_execution_resources().unwrap().filter_unused_builtins()
}

/// The Cairo arguments: `n_transactions`, then an array of `ProofFactsReference`s - a
/// (size, pointer) pair per transaction.
pub fn fold_block_proof_facts_args(per_transaction_proof_facts: &[&[Felt]]) -> Vec<EndpointArg> {
    let proof_facts_references = EndpointArg::Pointer(PointerArg::Composed(
        per_transaction_proof_facts
            .iter()
            .flat_map(|proof_facts| {
                [EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(proof_facts)]
            })
            .collect(),
    ));
    vec![EndpointArg::from(Felt::from(per_transaction_proof_facts.len())), proof_facts_references]
}
