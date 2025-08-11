use std::ops::AddAssign;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

#[cfg(test)]
#[path = "casm_hash_estimation_test.rs"]
mod casm_hash_estimation_test;

/// Represents an estimate of VM resources consumed when hashing CASM in the Starknet OS.
///
/// The variant indicates the hash function version used:
/// - V1Hash: Poseidon hash function.
/// - V2Hash: Blake hash function.
pub enum EstimatedExecutionResources {
    /// All execution resources are contained within `resources`.
    V1Hash { resources: ExecutionResources },

    /// Blake opcodes count is tracked separately, as they are not included in
    /// `ExecutionResources`.
    V2Hash { resources: ExecutionResources, blake_count: usize },
}

impl AddAssign<&ExecutionResources> for EstimatedExecutionResources {
    fn add_assign(&mut self, rhs: &ExecutionResources) {
        match self {
            EstimatedExecutionResources::V1Hash { resources } => *resources += rhs,
            EstimatedExecutionResources::V2Hash { resources, .. } => *resources += rhs,
        }
    }
}

impl AddAssign<&EstimatedExecutionResources> for EstimatedExecutionResources {
    fn add_assign(&mut self, rhs: &EstimatedExecutionResources) {
        match (self, rhs) {
            // V1 + V1: Only add resources
            (
                EstimatedExecutionResources::V1Hash { resources: left },
                EstimatedExecutionResources::V1Hash { resources: right },
            ) => {
                *left += right;
            }
            // V2 + V2: Add both resources and blake count
            (
                EstimatedExecutionResources::V2Hash {
                    resources: left_resources,
                    blake_count: left_blake,
                },
                EstimatedExecutionResources::V2Hash {
                    resources: right_resources,
                    blake_count: right_blake,
                },
            ) => {
                *left_resources += right_resources;
                *left_blake =
                    left_blake.checked_add(*right_blake).expect("Overflow in blake_count addition");
            }
            // Any mismatched variant
            _ => panic!("Cannot add EstimatedExecutionResources of different variants"),
        }
    }
}
