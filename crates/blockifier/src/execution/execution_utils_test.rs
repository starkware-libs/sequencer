use blake2s::encode_felts_to_u32s;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::bouncer::vm_resources_to_sierra_gas;
use crate::execution::execution_utils::blake_encoding::{N_U32S_BIG_FELT, N_U32S_SMALL_FELT};
use crate::execution::execution_utils::blake_estimation::BASE_STEPS_FULL_MSG;
use crate::execution::execution_utils::{
    compute_blake_hash_steps,
    cost_of_encode_felt252_data_and_calc_blake_hash,
    count_blake_opcode,
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
    assert_eq!(small_u32s.len(), N_U32S_SMALL_FELT);
    assert_eq!(big_u32s.len(), N_U32S_BIG_FELT);
}

/// Test the edge case of hashing an empty array of felt values.
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
    let gas = cost_of_encode_felt252_data_and_calc_blake_hash(
        0,
        0,
        VersionedConstants::latest_constants(),
        BouncerConfig::default().blake_weight,
    );
    let expected_gas = {
        let resources = ExecutionResources { n_steps: BASE_STEPS_FULL_MSG, ..Default::default() };
        vm_resources_to_sierra_gas(&resources, VersionedConstants::latest_constants())
    };
    assert_eq!(gas, expected_gas, "Unexpected gas value for zero-input hash");
}

// TODO(AvivG): Add tests for:
// - `compute_blake_hash_steps` simple cases (felts input).
// - `count_blake_opcode` simple cases (felts input).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` simple cases (felts input) (including partial
//   remainder).
// - `cost_of_encode_felt252_data_and_calc_blake_hash` compare against actual execution resources
//   from running a Cairo entry point (computing blake).
// - base steps costs - compare against actual execution resources by running on an empty input.
