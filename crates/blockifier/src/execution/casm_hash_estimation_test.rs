use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;

use crate::execution::casm_hash_estimation::EstimatedExecutionResources;

#[rstest]
#[should_panic]
#[case::add_v2_to_v1(
    EstimatedExecutionResources::V1 { resources: ExecutionResources::default() },
    EstimatedExecutionResources::V2 { resources: ExecutionResources::default(), blake_count: 0 }
)]
#[case::add_v1_to_v2(
    EstimatedExecutionResources::V2 { resources: ExecutionResources::default(), blake_count: 0 },
    EstimatedExecutionResources::V1 { resources: ExecutionResources::default() }
)]
fn add_estimated_resources_v2_to_v1_should_panic(
    #[case] mut first_resources: EstimatedExecutionResources,
    #[case] second_resources: EstimatedExecutionResources,
) {
    first_resources += &second_resources;
}
