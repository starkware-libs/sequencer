use std::collections::HashMap;
use std::ops::AddAssign;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::compiled_class_hash::HashVersion;

use crate::execution::contract_class::{
    EntryPointV1,
    EntryPointsByType,
    FeltSizeCount,
    NestedFeltCounts,
};
use crate::execution::execution_utils::{encode_and_blake_hash_resources, poseidon_hash_many_cost};

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

    pub fn resources(&self) -> &ExecutionResources {
        match self {
            EstimatedExecutionResources::V1Hash { resources } => resources,
            EstimatedExecutionResources::V2Hash { resources, .. } => resources,
        }
    }

    pub fn blake_count(&self) -> usize {
        match self {
            EstimatedExecutionResources::V2Hash { blake_count, .. } => *blake_count,
            _ => panic!("Cannot get blake count from V1Hash"),
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
            // Any mismatched variant
            _ => panic!("Cannot add EstimatedExecutionResources of different variants"),
        }
    }
}

impl From<(ExecutionResources, HashVersion)> for EstimatedExecutionResources {
    fn from((resources, hash_version): (ExecutionResources, HashVersion)) -> Self {
        match hash_version {
            HashVersion::V1 => EstimatedExecutionResources::V1Hash { resources },
            HashVersion::V2 => EstimatedExecutionResources::V2Hash { resources, blake_count: 0 },
        }
    }
}

/// Trait for estimating the Cairo execution resources consumed when running the
/// `compiled_class_hash` function in the Starknet OS.
///
/// Varied implementations of this trait correspond to a specific hash function used by
/// `compiled_class_hash`.
///
/// This provides resource estimates rather than exact values.
// TODO(AvivG): Remove allow once used.
#[allow(unused)]
trait EstimateCasmHashResources {
    /// Specifies the hash function variant that the estimate is for.
    fn hash_version(&self) -> HashVersion;

    /// Estimates the Cairo execution resources used when applying the hash function during CASM
    /// hashing.
    fn estimated_resources_of_hash_function(
        &mut self,
        _felt_count: FeltSizeCount,
    ) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources for `compiled_class_hash` in the
    /// Starknet OS.
    fn estimated_resources_of_compiled_class_hash(
        &mut self,
        _bytecode_segment_felt_sizes: &NestedFeltCounts,
        _entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Implement.
        EstimatedExecutionResources::new(self.hash_version())
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
pub(crate) struct CasmV1HashResourceEstimate {}

impl CasmV1HashResourceEstimate {
    /// Returns the estimated VM resources required for computing Casm hash (for Cairo 1 contracts).
    ///
    /// Note: the function focuses on the bytecode size, and currently ignores the cost handling the
    /// class entry points.
    /// Also, this function is not backward compatible.
    pub fn estimate_casm_poseidon_hash_computation_resources(
        bytecode_segment_lengths: &NestedFeltCounts,
    ) -> ExecutionResources {
        // The constants in this function were computed by running the Casm code on a few values
        // of `bytecode_segment_lengths`.
        match bytecode_segment_lengths {
            NestedFeltCounts::Leaf(length, _) => {
                // The entire contract is a single segment (old Sierra contracts).
                &ExecutionResources {
                    n_steps: 464,
                    n_memory_holes: 0,
                    builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 10)]),
                } + &poseidon_hash_many_cost(*length)
            }
            NestedFeltCounts::Node(segments) => {
                // The contract code is segmented by its functions.
                let mut execution_resources = ExecutionResources {
                    n_steps: 482,
                    n_memory_holes: 0,
                    builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 11)]),
                };
                let base_segment_cost = ExecutionResources {
                    n_steps: 25,
                    n_memory_holes: 2,
                    builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 1)]),
                };
                for segment in segments {
                    let NestedFeltCounts::Leaf(length, _) = segment else {
                        panic!(
                            "Estimating hash cost is only supported for segmentation depth at \
                             most 1."
                        );
                    };
                    execution_resources += &poseidon_hash_many_cost(*length);
                    execution_resources += &base_segment_cost;
                }
                execution_resources
            }
        }
    }
}

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
    fn hash_version(&self) -> HashVersion {
        HashVersion::V1
    }

    fn estimated_resources_of_hash_function(
        &mut self,
        felt_count: FeltSizeCount,
    ) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V1Hash {
            // TODO(AvivG): Consider inlining `poseidon_hash_many_cost` logic here.
            resources: poseidon_hash_many_cost(felt_count.n_felts()),
        }
    }
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
pub(crate) struct CasmV2HashResourceEstimate {}

impl CasmV2HashResourceEstimate {
    /// Cost to hash a single flat segment of `len` felts.
    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources {
        // All `len` inputs treated as “big” felts; no small-felt optimization here.
        encode_and_blake_hash_resources(felt_size_groups.large, felt_size_groups.small)
    }

    /// Cost to hash a multi-segment contract:
    fn node_cost(segs: &[NestedFeltCounts]) -> EstimatedExecutionResources {
        // TODO(AvivG): Add base estimation for node.
        let mut resources =
            EstimatedExecutionResources::from((ExecutionResources::default(), HashVersion::V2));

        // TODO(AvivG): Add base estimation of each segment. Could this be part of 'leaf_cost'?
        let segment_overhead = ExecutionResources::default();

        // For each segment, hash its felts.
        for seg in segs {
            match seg {
                NestedFeltCounts::Leaf(_, felt_size_groups) => {
                    resources += &segment_overhead;
                    resources += &Self::leaf_cost(felt_size_groups);
                }
                _ => {
                    panic!("Estimating hash cost only supports at most one level of segmentation.")
                }
            }
        }

        // Node‐level hash over (hash1, len1, hash2, len2, …): one segment hash (“big” felt))
        // and one segment length (“small” felt) per segment.
        resources += &encode_and_blake_hash_resources(segs.len(), segs.len());

        resources
    }

    /// Estimates the VM resources to compute the CASM Blake hash for a Cairo-1 contract:
    /// - Uses only bytecode size.
    pub fn estimate_casm_blake_hash_computation_resources(
        bytecode_segment_lengths: &NestedFeltCounts,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Currently ignores entry-point hashing costs.
        // TODO(AvivG): Missing base overhead estimation for compiled_class_hash.

        // Basic frame overhead.
        // TODO(AvivG): Once compiled_class_hash estimation is complete,
        // revisit whether this should be moved into
        // cost_of_encode_felt252_data_and_calc_blake_hash.
        let mut resources = EstimatedExecutionResources::from((
            ExecutionResources {
                n_steps: 0,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 3)]),
            },
            HashVersion::V2,
        ));

        // Add leaf vs node cost
        let added_resources = match &bytecode_segment_lengths {
            // Single-segment contract (e.g., older Sierra contracts).
            NestedFeltCounts::Leaf(_, felt_size_groups) => Self::leaf_cost(felt_size_groups),
            NestedFeltCounts::Node(segs) => Self::node_cost(segs),
        };

        resources += &added_resources;

        resources
    }
}

impl EstimateCasmHashResources for CasmV2HashResourceEstimate {
    fn hash_version(&self) -> HashVersion {
        HashVersion::V2
    }

    fn estimated_resources_of_hash_function(
        &mut self,
        _felt_count: FeltSizeCount,
    ) -> EstimatedExecutionResources {
        // TODO(AvivG): Use `cost_of_encode_felt252_data_and_calc_blake_hash` once it returns ER.
        EstimatedExecutionResources::new(HashVersion::V2)
    }
}
