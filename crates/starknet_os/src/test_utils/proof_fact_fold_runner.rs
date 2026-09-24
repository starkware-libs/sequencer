use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::PROOF_FACT_FOLD_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::proof_fact_fold::BLAKE2S_DIGEST_N_WORDS;
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};

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

pub fn run_cairo_processed_proof_output_digest(
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
