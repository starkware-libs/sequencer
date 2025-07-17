use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::execution_resources::GasAmount;

use super::{
    blake_cost,
    compute_blake_hash_steps,
    cost_of_encode_felt252_data_and_calc_blake_hash,
    count_blake_opcode,
};
use crate::test_utils::{test_gas_from_resources, test_gas_from_steps};
use crate::utils::u64_from_usize;

#[allow(clippy::assertions_on_constants)]
#[test]
fn test_blake_constants_validity() {
    // Ensure constants are reasonable
    assert!(blake_cost::N_U32S_MESSAGE > 0);
    assert!(blake_cost::N_U32S_BIG_FELT > blake_cost::N_U32S_SMALL_FELT);
    assert!(blake_cost::STEPS_BIG_FELT > blake_cost::STEPS_SMALL_FELT);
    assert!(blake_cost::BASE_STEPS_FULL_MSG > 0);
    assert!(blake_cost::BASE_STEPS_PARTIAL_MSG > 0);
    assert!(blake_cost::STEPS_PER_2_U32_REMINDER > 0);
}

#[test]
fn test_compute_blake_hash_steps_zero_inputs() {
    let steps = compute_blake_hash_steps(0, 0);
    // TODO(AvivG): Test input 0 in VM — was invalid when estimation function was written.
    assert_eq!(steps, blake_cost::BASE_STEPS_FULL_MSG);
}

#[test]
fn test_compute_blake_hash_steps_basic_cases() {
    // Test with only small felts
    let steps_1_small = compute_blake_hash_steps(0, 1);
    let expected_1_small = blake_cost::STEPS_SMALL_FELT
        + blake_cost::BASE_STEPS_PARTIAL_MSG
        + blake_cost::STEPS_PER_2_U32_REMINDER;
    assert_eq!(steps_1_small, expected_1_small);

    // Test with only big felts
    let steps_1_big = compute_blake_hash_steps(1, 0);
    let expected_1_big = blake_cost::STEPS_BIG_FELT
        + blake_cost::BASE_STEPS_PARTIAL_MSG
        + 4 * blake_cost::STEPS_PER_2_U32_REMINDER;
    assert_eq!(steps_1_big, expected_1_big);
}

#[test]
fn test_compute_blake_hash_steps_message_boundaries() {
    // Test exactly one full message (16 u32s)
    // 16 u32s = 2 big felts (8 u32s each)
    let steps_2_big = compute_blake_hash_steps(2, 0);
    let expected_2_big = 2 * blake_cost::STEPS_BIG_FELT + blake_cost::BASE_STEPS_FULL_MSG;
    assert_eq!(steps_2_big, expected_2_big);

    // 16 u32s = 8 small felts (2 u32s each)
    let steps_8_small = compute_blake_hash_steps(0, 8);
    let expected_8_small = 8 * blake_cost::STEPS_SMALL_FELT + blake_cost::BASE_STEPS_FULL_MSG;
    assert_eq!(steps_8_small, expected_8_small);

    // 16 u32s = 1 big felt + 4 small felts
    let steps_mixed_16 = compute_blake_hash_steps(1, 4);
    let expected_mixed_16 = blake_cost::STEPS_BIG_FELT
        + 4 * blake_cost::STEPS_SMALL_FELT
        + blake_cost::BASE_STEPS_FULL_MSG;
    assert_eq!(steps_mixed_16, expected_mixed_16);

    // Test multiple full messages (2 * 16 u32s = 4 big felts)
    let steps_4_big = compute_blake_hash_steps(4, 0);
    let expected_4_big = 4 * blake_cost::STEPS_BIG_FELT + blake_cost::BASE_STEPS_FULL_MSG;
    assert_eq!(steps_4_big, expected_4_big);

    // Test full + partial (18 u32s = 2 big felts + 1 small felt)
    let steps_full_partial = compute_blake_hash_steps(2, 1);
    let expected_full_partial = 2 * blake_cost::STEPS_BIG_FELT
        + blake_cost::STEPS_SMALL_FELT
        + blake_cost::BASE_STEPS_PARTIAL_MSG
        + blake_cost::STEPS_PER_2_U32_REMINDER;
    assert_eq!(steps_full_partial, expected_full_partial);
}

#[test]
fn test_count_blake_opcode_zero_inputs() {
    let opcodes = count_blake_opcode(0, 0);
    assert_eq!(opcodes, 0);
}

#[rstest]
// Opcode count = ceil(total_u32s / 16)
// big felt = 8 u32s, small felt = 2 u32s
#[case::no_opcodes(0, 0, 0)]
#[case::one_partial_opcode_small(1, 0, 1)]
#[case::one_partial_opcode_big(0, 1, 1)]
#[case::one_partial_opcode_mixed(1, 1, 1)]
#[case::one_full_opcode_big(2, 0, 1)]
#[case::one_full_opcode_small(0, 8, 1)]
#[case::two_partial_opcodes(2, 1, 2)]
#[case::two_full_opcodes(1, 12, 2)]
#[case::seven_opcodes(10, 10, 7)]
fn test_count_blake_opcode_parameterized(
    #[case] n_big_felts: usize,
    #[case] n_small_felts: usize,
    #[case] expected_opcodes: usize,
) {
    assert_eq!(count_blake_opcode(n_big_felts, n_small_felts), expected_opcodes);
}

// TODO(AvivG): Add test for 'cost_of_encode_felt252_data_and_calc_blake_hash' with non-zero inputs.
#[test]
fn test_cost_of_encode_felt252_data_and_calc_blake_hash_zero_inputs() {
    let gas = cost_of_encode_felt252_data_and_calc_blake_hash(0, 0, test_gas_from_resources);

    // Should have only base steps cost (no opcode cost)
    assert_eq!(gas, test_gas_from_steps(blake_cost::BASE_STEPS_FULL_MSG));
}
