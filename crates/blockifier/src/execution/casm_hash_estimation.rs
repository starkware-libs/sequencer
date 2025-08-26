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

    pub fn resources(&self) -> &ExecutionResources {
        match self {
            EstimatedExecutionResources::V1Hash { resources } => resources,
            EstimatedExecutionResources::V2Hash { resources, .. } => resources,
        }
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
    // Steps computation: call + return + hash_init + alloc_locals + assert + hash_update_single * 2
    // + call_hash_entry_points * 3 + call_bytecode_hash_node + call_hash_finalize.
    const BASE_COMPILED_CLASS_HASH_STEPS: usize = 54;
    // Steps computation: call + return + alloc_locals;
    const BASE_BYTECODE_HASH_NODE_STEPS: usize = 4;

    /// Estimates the Cairo execution resources used when applying the hash function during CASM
    /// hashing.
    fn estimated_resources_of_hash_function(
        _felt_size_groups: &FeltSizeCount,
    ) -> EstimatedExecutionResources;

    /// Creates an `EstimatedExecutionResources` from a given `ExecutionResources` matching the
    /// struct's hash function variant.
    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources for `compiled_class_hash` in the
    /// Starknet OS.
    // TODO(AvivG): Add estimation of entry points.
    fn estimated_resources_of_compiled_class_hash(
        bytecode_segment_felt_sizes: &NestedFeltCounts,
        _entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: Self::BASE_COMPILED_CLASS_HASH_STEPS,
            ..Default::default()
        });

        resources += &Self::estimated_resources_of_bytecode_hash_node(bytecode_segment_felt_sizes);

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
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: Self::BASE_BYTECODE_HASH_NODE_STEPS,
            ..Default::default()
        });

        // Add leaf vs node cost
        resources += &match bytecode_segment_felt_sizes {
            // Single-segment contract (e.g., older Sierra contracts).
            NestedFeltCounts::Leaf(_, felt_size_groups) => Self::leaf_cost(felt_size_groups),
            NestedFeltCounts::Node(segments) => Self::node_cost(segments),
        };

        resources
    }

    /// Estimates the Cairo execution resources for `bytecode_hahs_node` leaf case.
    ///
    /// The entire contract is a single segment (old Sierra contracts).
    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources for `bytecode_hahs_node` node case.
    ///
    /// The contract code is segmented by its functions.
    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources;
}

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
struct CasmV1HashResourceEstimate {}

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V1Hash { resources }
    }

    #[allow(unused)]
    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources {
        // TODO(AvivG): Move code from `estimate_casm_poseidon_hash_computation_resources`
        // `contract_class.rs` here and remove `#[allow(unused)]`.
        unimplemented!()
    }

    #[allow(unused)]
    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources {
        // TODO(AvivG): Move code from `estimate_casm_poseidon_hash_computation_resources`
        // `contract_class.rs` here and remove `#[allow(unused)]`.
        unimplemented!()
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
    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources {
        EstimatedExecutionResources::V2Hash { resources, blake_count: 0 }
    }

    #[allow(unused)]
    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources {
        // TODO(AvivG): Move code from `estimate_casm_hash_computation_resources`
        // `contract_class.rs` here and remove `#[allow(unused)]`.
        unimplemented!()
    }

    #[allow(unused)]
    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources {
        // TODO(AvivG): Move code from `estimate_casm_hash_computation_resources`
        // `contract_class.rs` here and remove `#[allow(unused)]`.
        unimplemented!()
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
