use blake2s::encode_felts_to_u32s;
use pretty_assertions::assert_eq;

use crate::execution::execution_utils::blake_cost::{
    BASE_STEPS_FULL_MSG,
    N_U32S_BIG_FELT,
    N_U32S_SMALL_FELT,
};
use crate::execution::execution_utils::{
    compute_blake_hash_steps,
    cost_of_encode_felt252_data_and_calc_blake_hash,
    count_blake_opcode,
};
use crate::test_utils::{
    create_big_felt,
    create_small_felt,
    test_gas_from_resources,
    test_gas_from_steps,
};

#[test]
fn test_u32_constants() {
    let small_felt = create_small_felt();
    let big_felt = create_big_felt();

    let small_u32s = encode_felts_to_u32s(vec![small_felt]);
    let big_u32s = encode_felts_to_u32s(vec![big_felt]);

    // Blake estimation constants should match the actual encoding.
    assert_eq!(small_u32s.len(), N_U32S_SMALL_FELT);
    assert_eq!(big_u32s.len(), N_U32S_BIG_FELT);
}

#[test]
fn test_zero_inputs() {
    // TODO(AvivG): Re-check this case in VM — input 0 was previously invalid when this estimation
    // logic was written.
    let steps = compute_blake_hash_steps(0, 0);
    assert_eq!(steps, BASE_STEPS_FULL_MSG, "Unexpected base step cost for zero inputs");

    // No opcodes should be emitted.
    let opcodes = count_blake_opcode(0, 0);
    assert_eq!(opcodes, 0, "Expected zero BLAKE opcodes for zero inputs");

    // Should result in base cost gas only (no opcode gas).
    let gas = cost_of_encode_felt252_data_and_calc_blake_hash(0, 0, test_gas_from_resources);
    let expected_gas = test_gas_from_steps(BASE_STEPS_FULL_MSG);
    assert_eq!(gas, expected_gas, "Unexpected gas value for zero-input hash");
}

// TODO(AvivG): Add tests for:
// - `compute_blake_hash_steps` simple cases (felts input).
// - `count_blake_opcode` simple cases (felts input).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` compare against actual execution resources
//   from running a Cairo entry point (computing blake).
