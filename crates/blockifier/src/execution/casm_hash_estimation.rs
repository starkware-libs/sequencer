use std::ops::AddAssign;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::compiled_class_hash::HashVersion;

use crate::execution::contract_class::{EntryPointV1, EntryPointsByType, NestedFeltCounts};

#[cfg(test)]
#[path = "casm_hash_estimation_test.rs"]
mod test;

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

impl EstimatedExecutionResources {
    pub fn new(hash_version: HashVersion) -> Self {
        match hash_version {
            HashVersion::V1 => {
                EstimatedExecutionResources::V1Hash { resources: ExecutionResources::default() }
            }
            HashVersion::V2 => EstimatedExecutionResources::V2Hash {
                resources: ExecutionResources::default(),
                blake_count: 0,
            },
        }
    }
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
            // V1 + V2 or V2 + V1: Panic
            (
                EstimatedExecutionResources::V1Hash { .. },
                EstimatedExecutionResources::V2Hash { .. },
            ) => {
                panic!("Cannot add V2 EstimatedExecutionResources to V1 variant")
            }
            (
                EstimatedExecutionResources::V2Hash { .. },
                EstimatedExecutionResources::V1Hash { .. },
            ) => {
                panic!("Cannot add V1 EstimatedExecutionResources to V2 variant")
            }
        }
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
trait ExecutionResourcesEstimator {
    fn hash_version(&self) -> HashVersion;

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
        EstimatedExecutionResources::new(self.hash_version())
    }
}
