use std::collections::HashMap;
use std::ops::AddAssign;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::execution_resources::GasAmount;

use crate::execution::contract_class::{
    EntryPointV1,
    EntryPointsByType,
    FeltSizeCount,
    NestedFeltCounts,
};
use crate::execution::execution_utils::{
    blake_estimation,
    count_blake_opcode,
    estimate_steps_of_encode_felt252_data_and_calc_blake_hash,
    poseidon_hash_many_cost,
};
use crate::utils::u64_from_usize;

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

    pub fn resources_ref(&self) -> &ExecutionResources {
        match self {
            EstimatedExecutionResources::V1Hash { resources } => resources,
            EstimatedExecutionResources::V2Hash { resources, .. } => resources,
        }
    }

    pub fn resources(&self) -> ExecutionResources {
        self.resources_ref().clone()
    }

    /// Returns the Blake opcode count.
    ///
    /// This is only defined for the V2 (Blake) variant.
    // TODO(AvivG): Consider returning 0 for V1 instead of panicking.
    pub fn blake_count(&self) -> usize {
        match self {
            EstimatedExecutionResources::V2Hash { blake_count, .. } => *blake_count,
            _ => panic!("Cannot get blake count from V1Hash"),
        }
    }

    pub fn to_sierra_gas<F>(
        &self,
        resources_to_gas_fn: F,
        blake_opcode_gas: Option<usize>,
    ) -> GasAmount
    where
        F: Fn(&ExecutionResources) -> GasAmount,
    {
        match self {
            EstimatedExecutionResources::V1Hash { resources } => resources_to_gas_fn(resources),
            EstimatedExecutionResources::V2Hash { resources, blake_count } => {
                let resources_gas = resources_to_gas_fn(resources);
                let blake_gas = blake_count
                    .checked_mul(blake_opcode_gas.unwrap())
                    .map(u64_from_usize)
                    .map(GasAmount)
                    .expect("Overflow computing Blake opcode gas.");

                resources_gas.checked_add_panic_on_overflow(blake_gas)
            }
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
pub trait EstimateCasmHashResources {
    // Base steps estimation for `bytecode_hash_internal_node` leaf case.
    const BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF_STEPS: usize;

    /// Creates an `EstimatedExecutionResources` from a given `ExecutionResources` matching the
    /// struct's hash function variant.
    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources used when applying the hash function during CASM
    /// hashing.
    fn estimated_resources_of_hash_function(
        _felt_size_groups: &FeltSizeCount,
    ) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources for `compiled_class_hash` in the
    /// Starknet OS.
    // TODO(AvivG): Add estimation of entry points.
    fn estimated_resources_of_compiled_class_hash(
        bytecode_segment_felt_sizes: &NestedFeltCounts,
        entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        // Base steps estimation for `compiled_class_hash`:
        // = call + return + hash_init + alloc_locals + assert + hash_update_single * 2 +
        // call_hash_entry_points * 3 + call_bytecode_hash_node + call_hash_finalize.
        const BASE_COMPILED_CLASS_HASH_STEPS: usize = 54;

        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: BASE_COMPILED_CLASS_HASH_STEPS,
            ..Default::default()
        });

        resources += &Self::estimated_resources_of_bytecode_hash_node(bytecode_segment_felt_sizes);
        resources +=
            &Self::estimated_resources_of_hash_entry_points(&entry_points_by_type.l1_handler);
        resources +=
            &Self::estimated_resources_of_hash_entry_points(&entry_points_by_type.external);
        resources +=
            &Self::estimated_resources_of_hash_entry_points(&entry_points_by_type.constructor);

        // Compute cost of `hash_finalize`: hash over (compiled_class_version, hash_entrypoints1,
        // hash_ep2, hash_ep3, hash_bytecode).
        let hash_finalize_data_len = 5;
        let hash_finalize_resources = Self::estimated_resources_of_hash_function(&FeltSizeCount {
            large: hash_finalize_data_len,
            small: 0,
        });
        resources += &hash_finalize_resources;

        resources
    }

    fn estimated_resources_of_bytecode_hash_node(
        bytecode_segment_felt_sizes: &NestedFeltCounts,
    ) -> EstimatedExecutionResources {
        // Base steps estimation for `bytecode_hash_node`: call + return + alloc_locals;
        const BASE_BYTECODE_HASH_NODE_STEPS: usize = 4;
        // Additional base steps estimation for `bytecode_hash_node` node case:
        // hash_init + call_bytecode_hash_internal_node + call_hash_finalize;
        const BASE_NODE_CASE_STEPS: usize = 15;

        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: BASE_BYTECODE_HASH_NODE_STEPS,
            ..Default::default()
        });

        // Add leaf vs node cost
        match bytecode_segment_felt_sizes {
            // The entire contract is a single segment (e.g., older Sierra contracts).
            NestedFeltCounts::Leaf(_, felt_size_groups) => {
                // In the leaf case, the entire segment is hashed together and returned.
                resources += &Self::estimated_resources_of_hash_function(felt_size_groups);
            }
            // The contract is segmented by its functions.
            NestedFeltCounts::Node(segments) => {
                // In the node case, `bytecode_hash_internal_node` is called.
                resources += &Self::from_resources(ExecutionResources {
                    n_steps: BASE_NODE_CASE_STEPS,
                    ..Default::default()
                });

                resources +=
                    &Self::estimated_resources_of_bytecode_hash_internal_node_leaf_case(segments);
            }
        };

        resources
    }

    /// Estimates the Cairo execution resources for a `bytecode_hash_internal_node` leaf case.
    ///
    /// The contract code is segmented by its functions, and each function is a single segment
    /// (no further segmentation).
    ///
    /// `bytecode_hash_internal_node` is applied recursively until all segments are hashed.
    fn estimated_resources_of_bytecode_hash_internal_node_leaf_case(
        bytecode_segment_felt_sizes: &[NestedFeltCounts],
    ) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources::default());

        let bytecode_hash_internal_node_overhead = ExecutionResources {
            n_steps: Self::BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF_STEPS,
            ..Default::default()
        };

        // For each segment, hash its felts.
        for seg in bytecode_segment_felt_sizes {
            match seg {
                NestedFeltCounts::Leaf(_, felt_size_groups) => {
                    resources += &bytecode_hash_internal_node_overhead;
                    resources += &Self::estimated_resources_of_hash_function(felt_size_groups);
                }
                _ => {
                    panic!("Estimating hash cost only supports at most one level of segmentation.")
                }
            }
        }

        // Computes cost of `hash_finalize`: a hash over (hash1, len1, hash2, len2, …): one
        // segment hash (“big” felt) and one segment length (“small” felt) per segment.
        resources += &Self::estimated_resources_of_hash_function(&FeltSizeCount {
            large: bytecode_segment_felt_sizes.len(),
            small: bytecode_segment_felt_sizes.len(),
        });

        resources
    }

    fn estimated_resources_of_hash_entry_points(
        entry_points: &[EntryPointV1],
    ) -> EstimatedExecutionResources {
        // Base steps for `hash_entry_points`:
        // 21 = hash_init + call_hash_entry_points_inner + call_hash_finalize + hash_update_single +
        // return;
        const BASE_HASH_ENTRY_POINTS_STEPS: usize = 21;

        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: BASE_HASH_ENTRY_POINTS_STEPS,
            ..Default::default()
        });

        for entry_point in entry_points {
            resources += &Self::estimated_resources_of_hash_entry_points_inner(entry_point);
        }

        // Computes cost of `hash_finalize`: a hash over (selector1, offset1, selector2, offset2,
        // ...). Each entry point has a selector (big felt) and an offset (small felt).
        // somethis with builtins make the large *2.
        resources += &Self::estimated_resources_of_hash_function(&FeltSizeCount {
            large: entry_points.len() + entry_points.len(),
            small: entry_points.len(),
        });

        resources
    }

    fn estimated_resources_of_hash_entry_points_inner(
        entry_point: &EntryPointV1,
    ) -> EstimatedExecutionResources {
        // Base steps for `hash_entry_points_inner`:
        // 27 = if + hash_update_single * 2 + call_hash_update_with_nested_hash +
        // call_hash_entry_points_inner;
        const BASE_HASH_ENTRY_POINTS_INNER_STEPS: usize = 27;
        // Base steps for `hash_update_with_nested_hash`:
        // 3 = call_hash_update_single + return;
        const BASE_HASH_UPDATE_WITH_NESTED_HASH_STEPS: usize = 3;

        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: BASE_HASH_ENTRY_POINTS_INNER_STEPS,
            ..Default::default()
        });

        // compute cost of `hash_update_with_nested_hash`
        let base_resources_of_hash_update_with_nested_hash = ExecutionResources {
            n_steps: BASE_HASH_UPDATE_WITH_NESTED_HASH_STEPS,
            ..Default::default()
        };
        resources += &base_resources_of_hash_update_with_nested_hash;

        // Builtin list contain both small and big felts—we treat all as big for simplicity.
        let resources_of_hash_update_with_nested_hash =
            &Self::estimated_resources_of_hash_function(&FeltSizeCount {
                large: entry_point.builtins.len(),
                small: 0,
            });

        resources += resources_of_hash_update_with_nested_hash;

        resources
    }
}

/// Estimates the VM resources to compute the CASM V1 (Poseidon) hash for a Cairo-1 contract.
///
/// Note: this function is not backward compatible.
pub struct CasmV1HashResourceEstimate {}

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
    // Base steps estimation for `bytecode_hash_internal_node` leaf case.
    // Computed based on running multiple contracts with different bytecode segment structures.
    const BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF_STEPS: usize = 18;

    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V1Hash { resources }
    }

    fn estimated_resources_of_hash_function(
        felt_size_groups: &FeltSizeCount,
    ) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V1Hash {
            // TODO(AvivG): Consider inlining `poseidon_hash_many_cost` logic here.
            resources: poseidon_hash_many_cost(felt_size_groups.n_felts()),
        }
    }
}

pub struct CasmV2HashResourceEstimate {}

impl EstimateCasmHashResources for CasmV2HashResourceEstimate {
    // Base steps estimation for `bytecode_hash_internal_node` leaf case:
    // = if*2 + return + alloc_local + if + let + tempvar * 2 + hash_update_single * 2(=8 +6) +
    // + call_bytecode_hash_internal_node;
    // Verified by running multiple contracts with different bytecode segment structures.
    const BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF_STEPS: usize = 30;

    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V2Hash { resources, blake_count: 0 }
    }

    /// Estimates resource usage for `encode_felt252_data_and_calc_blake_hash` in the Starknet OS.
    ///
    /// # Encoding Details
    /// - Small felts → 2 `u32`s each; Big felts → 8 `u32`s each.
    /// - Each felt requires one `range_check` operation.
    ///
    /// # Returns:
    /// - `ExecutionResources`: VM resource usage (e.g., n_steps, range checks).
    /// - `usize`: number of Blake opcodes used, accounted for separately as those are not reported
    ///   via `ExecutionResources`.
    fn estimated_resources_of_hash_function(
        felt_size_groups: &FeltSizeCount,
    ) -> EstimatedExecutionResources {
        let n_steps = estimate_steps_of_encode_felt252_data_and_calc_blake_hash(felt_size_groups);
        let builtin_instance_counter = match felt_size_groups.n_felts() {
            // The empty case does not use builtins at all.
            0 => HashMap::new(),
            // One `range_check` per input felt to validate its size + Overhead for the non empty
            // case.
            _ => HashMap::from([(
                BuiltinName::range_check,
                felt_size_groups.n_felts() + blake_estimation::BASE_RANGE_CHECK_NON_EMPTY,
            )]),
        };

        let resources = ExecutionResources { n_steps, n_memory_holes: 0, builtin_instance_counter };

        EstimatedExecutionResources::V2Hash {
            resources,
            blake_count: count_blake_opcode(felt_size_groups),
        }
    }
}
