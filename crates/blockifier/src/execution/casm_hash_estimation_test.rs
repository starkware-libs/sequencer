use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;

use crate::execution::casm_hash_estimation::EstimatedExecutionResources;

#[rstest]
#[should_panic(expected = "Cannot add V2 EstimatedExecutionResources to V1 variant")]
#[case::add_v2_to_v1(
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() },
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 }
)]
#[should_panic(expected = "Cannot add V1 EstimatedExecutionResources to V2 variant")]
#[case::add_v1_to_v2(
    EstimatedExecutionResources::V2Hash { resources: ExecutionResources::default(), blake_count: 0 },
    EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() }
)]
fn add_estimated_resources_should_panic(
    #[case] mut first_resources: EstimatedExecutionResources,
    #[case] second_resources: EstimatedExecutionResources,
) {
    first_resources += &second_resources;
}
