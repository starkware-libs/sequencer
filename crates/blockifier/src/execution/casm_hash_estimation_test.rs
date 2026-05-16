use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use starknet_types_core::hash::Blake2Felt252;

use crate::execution::call_info::OpcodeName;
use crate::execution::casm_hash_estimation::expected::{
    BASE_STEPS_FULL_MSG_EXPECT,
    BASE_STEPS_PARTIAL_MSG_EXPECT,
    STEPS_DISCOUNT_PER_FULL_MSG_EXPECT,
    STEPS_EMPTY_INPUT_EXPECT,
    STEPS_PER_LARGE_FELT_EXPECT,
    STEPS_PER_SMALL_FELT_EXPECT,
};
use crate::execution::casm_hash_estimation::{
    CasmV2HashResourceEstimate,
    EstimateCasmHashResources,
};
use crate::execution::contract_class::FeltSizeCount;

#[test]
fn test_u32_constants() {
    let small_felt = Blake2Felt252::SMALL_THRESHOLD - 1_u64;
    let large_felt = Blake2Felt252::SMALL_THRESHOLD;

    let small_u32s = Blake2Felt252::encode_felts_to_u32s(&[small_felt]);
    let large_u32s = Blake2Felt252::encode_felts_to_u32s(&[large_felt]);

    // Blake estimation constants should match the actual encoding.
    assert_eq!(small_u32s.len(), CasmV2HashResourceEstimate::U32_WORDS_PER_SMALL_FELT);
    assert_eq!(large_u32s.len(), CasmV2HashResourceEstimate::U32_WORDS_PER_LARGE_FELT);
}

/// Assert consistency between the blake constants used in production and the expected values
/// reproduced and auto-fixed by tests.
#[test]
fn test_expected_constants() {
    macro_rules! assert_eq_expect {
        ($expect:expr, $actual:ident) => {
            assert_eq!(
                $expect.data().parse::<usize>().unwrap(),
                CasmV2HashResourceEstimate::$actual,
                "Expect constant {} is not equal to constant CasmV2HashResourceEstimate::{} ({} \
                 != {}). Fix the latter (manually) to match the former.",
                stringify!($expect),
                stringify!($actual),
                $expect.data().parse::<usize>().unwrap(),
                CasmV2HashResourceEstimate::$actual,
            );
        };
    }
    assert_eq_expect!(STEPS_EMPTY_INPUT_EXPECT, STEPS_EMPTY_INPUT);
    assert_eq_expect!(STEPS_PER_LARGE_FELT_EXPECT, STEPS_PER_LARGE_FELT);
    assert_eq_expect!(STEPS_PER_SMALL_FELT_EXPECT, STEPS_PER_SMALL_FELT);
    assert_eq_expect!(BASE_STEPS_FULL_MSG_EXPECT, BASE_STEPS_FULL_MSG);
    assert_eq_expect!(BASE_STEPS_PARTIAL_MSG_EXPECT, BASE_STEPS_PARTIAL_MSG);
    assert_eq_expect!(STEPS_DISCOUNT_PER_FULL_MSG_EXPECT, STEPS_DISCOUNT_PER_FULL_MSG);
}

/// Test the edge case of hashing an empty array of felt values.
#[test]
fn test_zero_inputs() {
    let steps =
        CasmV2HashResourceEstimate::estimate_steps_of_encode_felt252_data_and_calc_blake_hash(
            &FeltSizeCount::default(),
        );
    assert_eq!(
        steps,
        CasmV2HashResourceEstimate::STEPS_EMPTY_INPUT,
        "Unexpected base step cost for zero inputs"
    );

    // No opcodes should be emitted.
    let opcodes = FeltSizeCount::default().blake_opcode_count();
    assert_eq!(opcodes, 0, "Expected zero BLAKE opcodes for zero inputs");

    // Should result in base cost only (no opcode cost).
    let resources =
        CasmV2HashResourceEstimate::estimated_resources_of_hash_function(&FeltSizeCount::default());
    let expected = ExecutionResources {
        n_steps: CasmV2HashResourceEstimate::STEPS_EMPTY_INPUT,
        ..Default::default()
    };
    assert_eq!(resources.vm_resources, expected, "Unexpected resources values for zero-input hash");
    assert_eq!(
        *resources.opcode_instance_counter.get(&OpcodeName::blake).unwrap_or(&0),
        0,
        "Expected zero BLAKE opcodes for zero inputs"
    );
}
