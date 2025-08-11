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
