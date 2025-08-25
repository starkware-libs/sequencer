use std::collections::HashMap;

use blake2s::encode_felts_to_u32s;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_types_core::felt::Felt;

use crate::execution::casm_hash_estimation::{
    CasmV2HashResourceEstimate,
    EstimateCasmHashResources,
    EstimatedExecutionResources,
};
use crate::execution::contract_class::FeltSizeCount;

impl EstimatedExecutionResources {
    /// Constructs an `EstimatedExecutionResources` for the V1 (Poseidon) hash function.
    pub fn v1(
        n_steps: usize,
        n_memory_holes: usize,
        builtin_instance_counter: HashMap<BuiltinName, usize>,
    ) -> Self {
        Self::V1Hash {
            resources: ExecutionResources { n_steps, n_memory_holes, builtin_instance_counter },
        }
    }

    /// Constructs an `EstimatedExecutionResources` for the V2 (Blake) hash function.
    pub fn v2(
        n_steps: usize,
        n_memory_holes: usize,
        builtin_instance_counter: HashMap<BuiltinName, usize>,
        blake_count: usize,
    ) -> Self {
        Self::V2Hash {
            resources: ExecutionResources { n_steps, n_memory_holes, builtin_instance_counter },
            blake_count,
        }
    }
}

#[rstest]
#[case::add_v2_to_v1(
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() },
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 }
)]
#[case::add_v1_to_v2(
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 },
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() }
)]
#[should_panic(expected = "Cannot add EstimatedExecutionResources of different variants")]
fn add_assign_estimated_resources_panics_on_variant_mismatch(
    #[case] mut first_resources: EstimatedExecutionResources,
    #[case] second_resources: EstimatedExecutionResources,
) {
    first_resources += &second_resources;
}

#[rstest]
#[case::v1_to_v1(
    EstimatedExecutionResources::v1(1, 1, HashMap::from([(BuiltinName::poseidon, 2)])),
    EstimatedExecutionResources::v1(1, 1, HashMap::from([(BuiltinName::poseidon, 1)])),
    // Expected execution resources.
    ExecutionResources {
        n_steps: 2,
        n_memory_holes: 2,
        builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 3)]),
    },
    // Expected blake count.
    None,
)]
#[case::v2_to_v2(
    EstimatedExecutionResources::v2(1, 1, HashMap::from([(BuiltinName::range_check, 2)]), 2),
    EstimatedExecutionResources::v2(1, 1, HashMap::from([(BuiltinName::range_check, 1)]), 1),
    // Expected execution resources.
    ExecutionResources {
        n_steps: 2,
        n_memory_holes: 2,
        builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 3)]),
    },
    // Expected blake count.
    Some(3),
)]
fn add_assign_estimated_resources_success(
    #[case] mut first_resources: EstimatedExecutionResources,
    #[case] second_resources: EstimatedExecutionResources,
    #[case] expected_resources: ExecutionResources,
    #[case] expected_blake_count: Option<usize>,
) {
    first_resources += &second_resources;

    // Check that the result is as expected.
    match first_resources {
        EstimatedExecutionResources::V1Hash { resources } => {
            assert_eq!(resources, expected_resources);
        }
        EstimatedExecutionResources::V2Hash { resources, blake_count } => {
            assert_eq!(resources, expected_resources);
            assert_eq!(Some(blake_count), expected_blake_count);
        }
    }
}

#[test]
fn test_u32_constants() {
    // Small value < 2^63, will encode to 2 u32s.
    let small_felt = Felt::ONE;
    // Large value >= 2^63, will encode to 8 u32s (Just above 2^63).
    let big_felt = Felt::from_hex_unchecked("8000000000000001");

    let small_u32s = encode_felts_to_u32s(vec![small_felt]);
    let big_u32s = encode_felts_to_u32s(vec![big_felt]);

    // Blake estimation constants should match the actual encoding.
    assert_eq!(small_u32s.len(), CasmV2HashResourceEstimate::U32_WORDS_PER_SMALL_FELT);
    assert_eq!(big_u32s.len(), CasmV2HashResourceEstimate::U32_WORDS_PER_LARGE_FELT);
}

/// Test the edge case of hashing an empty array of felt values.
#[test]
fn test_zero_inputs() {
    // logic was written.
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
    assert_eq!(resources.resources(), &expected, "Unexpected resources values for zero-input hash");
    assert_eq!(resources.blake_count(), 0, "Expected zero BLAKE opcodes for zero inputs");
}
