use blake2s::encode_felts_to_u32s;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use starknet_types_core::felt::Felt;

use crate::execution::casm_hash_estimation::{
    CasmV2HashResourceEstimate,
    EstimateCasmHashResources,
};
use crate::execution::contract_class::FeltSizeCount;
use crate::execution::execution_utils::blake_estimation::STEPS_EMPTY_INPUT;
use crate::execution::execution_utils::{
    blake_encoding,
    count_blake_opcode,
    estimate_steps_of_encode_felt252_data_and_calc_blake_hash,
};

#[test]
fn test_u32_constants() {
    // Small value < 2^63, will encode to 2 u32s.
    let small_felt = Felt::ONE;
    // Large value >= 2^63, will encode to 8 u32s (Just above 2^63).
    let big_felt = Felt::from_hex_unchecked("8000000000000001");

    let small_u32s = encode_felts_to_u32s(vec![small_felt]);
    let big_u32s = encode_felts_to_u32s(vec![big_felt]);

    // Blake estimation constants should match the actual encoding.
    assert_eq!(small_u32s.len(), blake_encoding::U32_WORDS_PER_SMALL_FELT);
    assert_eq!(big_u32s.len(), blake_encoding::U32_WORDS_PER_LARGE_FELT);
}

/// Test the edge case of hashing an empty array of felt values.
#[test]
fn test_zero_inputs() {
    // logic was written.
    let steps =
        estimate_steps_of_encode_felt252_data_and_calc_blake_hash(&FeltSizeCount::default());
    assert_eq!(steps, STEPS_EMPTY_INPUT, "Unexpected base step cost for zero inputs");

    // No opcodes should be emitted.
    let opcodes = count_blake_opcode(&FeltSizeCount::default());
    assert_eq!(opcodes, 0, "Expected zero BLAKE opcodes for zero inputs");

    // Should result in base cost only (no opcode cost).
    let resources =
        CasmV2HashResourceEstimate::estimated_resources_of_hash_function(&FeltSizeCount::default());
    let expected = ExecutionResources { n_steps: STEPS_EMPTY_INPUT, ..Default::default() };
    assert_eq!(resources.resources(), &expected, "Unexpected resources values for zero-input hash");
    assert_eq!(resources.blake_count(), 0, "Expected zero BLAKE opcodes for zero inputs");
}

// TODO(AvivG): Add tests for:
// - `estimate_steps_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input).
// - `count_blake_opcode` simple cases (felts input).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input) (including partial
//   remainder).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` compare against actual execution resources
//   from running a Cairo entry point (computing blake).
// - base steps costs - compare against actual execution resources by running on an empty input.
