use std::ops::AddAssign;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use crate::execution::contract_class::{EntryPointV1, EntryPointsByType, NestedFeltCounts};

#[cfg(test)]
#[path = "casm_hash_estimation_test.rs"]
mod test;

/// Represents an estimate of VM resources used by Cairo functions in the Starknet OS.
#[derive(Debug)]
pub enum EstimatedExecutionResources {
    V1 { resources: ExecutionResources },
    V2 { resources: ExecutionResources, blake_count: usize },
}

impl AddAssign<&ExecutionResources> for EstimatedExecutionResources {
    fn add_assign(&mut self, rhs: &ExecutionResources) {
        match self {
            EstimatedExecutionResources::V1 { resources } => *resources += rhs,
            EstimatedExecutionResources::V2 { resources, .. } => *resources += rhs,
        }
    }
}

impl AddAssign<&EstimatedExecutionResources> for EstimatedExecutionResources {
    fn add_assign(&mut self, rhs: &EstimatedExecutionResources) {
        match (self, rhs) {
            // V1 + V1: Only add resources
            (
                EstimatedExecutionResources::V1 { resources: left },
                EstimatedExecutionResources::V1 { resources: right },
            ) => {
                *left += right;
            }
            // V2 + V2: Add both resources and blake count
            (
                EstimatedExecutionResources::V2 {
                    resources: left_resources,
                    blake_count: left_blake,
                },
                EstimatedExecutionResources::V2 {
                    resources: right_resources,
                    blake_count: right_blake,
                },
            ) => {
                *left_resources += right_resources;
                *left_blake =
                    left_blake.checked_add(*right_blake).expect("Overflow in blake_count addition");
            }
            // V1 + V2 or V2 + V1: Panic
            (EstimatedExecutionResources::V1 { .. }, EstimatedExecutionResources::V2 { .. }) => {
                panic!("Cannot add V2 EstimatedExecutionResources to V1 variant")
            }
            (EstimatedExecutionResources::V2 { .. }, EstimatedExecutionResources::V1 { .. }) => {
                panic!("Cannot add V1 EstimatedExecutionResources to V2 variant")
            }
        }
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
struct ExecutionResourcesEstimatorV1 {}

impl ExecutionResourcesEstimator for ExecutionResourcesEstimatorV1 {
    /// Creates `EstimatedExecutionResources` for the V1 hash version.
    fn create_resources(&self) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V1 { resources: ExecutionResources::default() }
    }

    fn cost_of_hash_function(
        &mut self,
        _felt_count: NestedFeltCounts,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Implement by calling 'poseidon_hash_many_cost'.
        self.create_resources()
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
struct ExecutionResourcesEstimatorV2 {}

impl ExecutionResourcesEstimator for ExecutionResourcesEstimatorV2 {
    /// Creates `EstimatedExecutionResources` for the V2 hash version.
    fn create_resources(&self) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V2 { resources: ExecutionResources::default(), blake_count: 0 }
    }

    fn cost_of_hash_function(
        &mut self,
        _felt_count: NestedFeltCounts,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Implement by calling 'cost_of_encode_felt252_data_and_calc_blake_hash'.
        self.create_resources()
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
trait ExecutionResourcesEstimator {
    /// Creates `EstimatedExecutionResources` for the current hash version.
    fn create_resources(&self) -> EstimatedExecutionResources;

    /// Estimates resources used by the hash function for the current hash version.
    fn cost_of_hash_function(
        &mut self,
        _felt_count: NestedFeltCounts,
    ) -> EstimatedExecutionResources;

    /// Estimates resources used by the `compiled_class_hash` Cairo function used in the Starknet
    /// OS.
    fn cost_of_compiled_class_hash(
        &mut self,
        _bytecode_segment_felt_sizes: &NestedFeltCounts,
        _entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Implement.
        self.create_resources()
    }
}
