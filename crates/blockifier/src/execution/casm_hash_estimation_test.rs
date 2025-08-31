use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;

use crate::execution::casm_hash_estimation::EstimatedExecutionResources;

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
