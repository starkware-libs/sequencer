use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;

use crate::execution::casm_hash_estimation::EstimatedExecutionResources;

#[rstest]
#[should_panic(expected = "Cannot add EstimatedExecutionResources of different variants")]
#[case::add_v2_to_v1(
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() },
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 }
)]
#[should_panic(expected = "Cannot add EstimatedExecutionResources of different variants")]
#[case::add_v1_to_v2(
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 },
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() }
)]
#[case::add_v1_to_v1(
    EstimatedExecutionResources::V1Hash {
        resources: ExecutionResources {
            n_steps: 1,
            n_memory_holes: 1,
            builtin_instance_counter: HashMap::from([
                (BuiltinName::poseidon, 2),
            ]),
        },
    },
    EstimatedExecutionResources::V1Hash {
        resources: ExecutionResources {
            n_steps: 1,
            n_memory_holes: 1,
            builtin_instance_counter: HashMap::from([
                (BuiltinName::poseidon, 1),
            ]),
        },
    }
)]
#[case::add_v2_to_v2(
    EstimatedExecutionResources::V2Hash {
        resources: ExecutionResources {
            n_steps: 1,
            n_memory_holes: 1,
            builtin_instance_counter: HashMap::from([
                (BuiltinName::range_check, 2),
            ]),
        },
        blake_count: 2,
    },
    EstimatedExecutionResources::V2Hash {
        resources: ExecutionResources {
            n_steps: 1,
            n_memory_holes: 1,
            builtin_instance_counter: HashMap::from([
                (BuiltinName::range_check, 1),
            ]),
        },
        blake_count: 1,
    }
)]
fn add_assign_estimated_execution_resources(
    #[case] mut first_resources: EstimatedExecutionResources,
    #[case] second_resources: EstimatedExecutionResources,
) {
    first_resources += &second_resources;
}
