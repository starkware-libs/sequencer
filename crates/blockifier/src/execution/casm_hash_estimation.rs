use std::collections::HashMap;
use std::ops::AddAssign;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::execution_resources::GasAmount;

use crate::blockifier_versioned_constants::{BuiltinGasCosts, VersionedConstants};
use crate::bouncer::vm_resources_to_gas;
use crate::execution::contract_class::{
    EntryPointV1,
    EntryPointsByType,
    FeltSizeCount,
    NestedFeltCounts,
};
use crate::execution::execution_utils::poseidon_hash_many_cost;
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
    /// For V1Hash, returns 0.
    pub fn blake_count(&self) -> usize {
        match self {
            EstimatedExecutionResources::V2Hash { blake_count, .. } => *blake_count,
            EstimatedExecutionResources::V1Hash { .. } => 0,
        }
    }

    pub fn to_gas(
        &self,
        builtin_gas_cost: &BuiltinGasCosts,
        blake_opcode_gas: usize,
        versioned_constants: &VersionedConstants,
    ) -> GasAmount {
        let resources_gas =
            vm_resources_to_gas(self.resources_ref(), builtin_gas_cost, versioned_constants);

        let blake_gas = self
            .blake_count()
            .checked_mul(blake_opcode_gas)
            .map(u64_from_usize)
            .map(GasAmount)
            .expect("Overflow computing Blake opcode gas.");

        resources_gas.checked_add_panic_on_overflow(blake_gas)
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
    // Estimated fixed Cairo steps for `bytecode_hash_internal_node` leaf case.
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
        _entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        // Estimated fixed Cairo steps for `compiled_class_hash` (independent of input):
        // 54 = `call` + `return` + `hash_init` + `alloc_locals` + `assert` +
        // 2*`hash_update_single` + 3*`call_hash_entry_points` + `call_bytecode_hash_node` +
        // `call_hash_finalize`.
        const BASE_COMPILED_CLASS_HASH_STEPS: usize = 54;

        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: BASE_COMPILED_CLASS_HASH_STEPS,
            ..Default::default()
        });

        resources += &Self::estimated_resources_of_bytecode_hash_node(bytecode_segment_felt_sizes);

        // Compute cost of `hash_finalize`: hash over (compiled_class_version, hash_ep1, hash_ep2,
        // hash_ep3, hash_bytecode).
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
        // Estimated fixed Cairo steps for `bytecode_hash_node` (independent of input):
        // 4 = `call` + `return` + `alloc_locals`.
        const BASE_BYTECODE_HASH_NODE_STEPS: usize = 4;
        // Additional estimated fixed Cairo steps for `bytecode_hash_node` node case:
        // 15 = `hash_init` + `call_bytecode_hash_internal_node` + `call_hash_finalize`.
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

        // Compute cost of `hash_finalize`: hash over (hash1, len1, hash2, len2, …).
        // One segment hash (“big” felt) and one segment length (“small” felt) per segment.
        resources += &Self::estimated_resources_of_hash_function(&FeltSizeCount {
            large: bytecode_segment_felt_sizes.len(),
            small: bytecode_segment_felt_sizes.len(),
        });

        resources
    }
}

/// Estimates the VM resources to compute the CASM V1 (Poseidon) hash for a Cairo-1 contract.
///
/// Note: this estimation is not backward compatible.
pub struct CasmV1HashResourceEstimate {}

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
    // Estimated fixed Cairo steps for `bytecode_hash_internal_node` leaf case.
    // Computed across running multiple contracts with different bytecode segment structures.
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

impl CasmV2HashResourceEstimate {
    // Constants that define how felts are encoded into u32s for BLAKE hashing.
    // Number of `u32` words a large felt expands into.
    pub(crate) const U32_WORDS_PER_LARGE_FELT: usize = 8;
    // Number of `u32` words a small felt expands into.
    pub(crate) const U32_WORDS_PER_SMALL_FELT: usize = 2;
    // Input for Blake hash function is a sequence of 16 `u32` words.
    pub(crate) const U32_WORDS_PER_MESSAGE: usize = 16;

    // Base number of VM steps applied when the input to Blake hashing is empty.
    // Determined empirically by running `encode_felt252_data_and_calc_blake_hash` on empty input.
    pub(crate) const STEPS_EMPTY_INPUT: usize = 170;

    /// Estimates the number of VM steps required to hash the given felts with Blake in Starknet OS.
    ///
    /// - Each small felt unpacks into 2 `u32`s.
    /// - Each large felt unpacks into 8 `u32`s.
    /// - Adds a base cost depending on whether the total encoded `u32` sequence fits exactly into
    ///   full 16-`u32` Blake messages.
    fn estimate_steps_of_encode_felt252_data_and_calc_blake_hash(
        felt_size_groups: &FeltSizeCount,
    ) -> usize {
        // The constants used are empirical, based on running
        // `encode_felt252_data_and_calc_blake_hash` on combinations of large and small
        // felts. VM steps per large felt.
        const STEPS_PER_LARGE_FELT: usize = 45;
        // VM steps per small felt.
        const STEPS_PER_SMALL_FELT: usize = 15;
        // Base overhead when input exactly fills a 16-u32 Blake message.
        const BASE_STEPS_FULL_MSG: usize = 217;
        // Base overhead when the input leaves a remainder (< 16 u32s) for a Blake message.
        const BASE_STEPS_PARTIAL_MSG: usize = 195;
        // Extra VM steps added per 2-u32 remainder in partial Blake messages.
        const STEPS_PER_2_U32_REMINDER: usize = 3;

        let encoded_u32_len = felt_size_groups.encoded_u32_len();
        if encoded_u32_len == 0 {
            // The empty input case is a special case.
            return Self::STEPS_EMPTY_INPUT;
        }

        // Adds a base cost depending on whether the total fits exactly into full 16-u32 messages.
        let base_steps = if encoded_u32_len % Self::U32_WORDS_PER_MESSAGE == 0 {
            BASE_STEPS_FULL_MSG
        } else {
            // This computation is based on running blake2s with different inputs.
            // Note: all inputs expand to an even number of u32s --> `rem_u32s` is always even.
            BASE_STEPS_PARTIAL_MSG
                + (encoded_u32_len % Self::U32_WORDS_PER_MESSAGE / 2) * STEPS_PER_2_U32_REMINDER
        };

        base_steps
            + felt_size_groups.large * STEPS_PER_LARGE_FELT
            + felt_size_groups.small * STEPS_PER_SMALL_FELT
    }
}

impl EstimateCasmHashResources for CasmV2HashResourceEstimate {
    // Estimated fixed Cairo steps for `bytecode_hash_internal_node` (leaf case):
    // 30 = 2*`if` + `return` + `alloc_locals` + `let` + 2*`tempvar` + 2*`hash_update_single` +
    // `call_bytecode_hash_internal_node`. Verified across running multiple contracts.
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
        // One-time additional `range_check` required for `encode_felt252_data_and_calc_blake_hash`
        // execution when the input is non-empty.
        const BASE_RANGE_CHECK_NON_EMPTY: usize = 3;

        let n_steps =
            CasmV2HashResourceEstimate::estimate_steps_of_encode_felt252_data_and_calc_blake_hash(
                felt_size_groups,
            );
        let builtin_instance_counter = match felt_size_groups.n_felts() {
            // The empty case does not use builtins at all.
            0 => HashMap::new(),
            // One `range_check` per input felt to validate its size + Overhead for the non empty
            // case.
            _ => HashMap::from([(
                BuiltinName::range_check,
                felt_size_groups.n_felts() + BASE_RANGE_CHECK_NON_EMPTY,
            )]),
        };

        let resources = ExecutionResources { n_steps, n_memory_holes: 0, builtin_instance_counter };

        EstimatedExecutionResources::V2Hash {
            resources,
            blake_count: felt_size_groups.blake_opcode_count(),
        }
    }
}
