use std::collections::HashMap;
use std::ops::AddAssign;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::execution_resources::GasAmount;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::bouncer::vm_resources_to_sierra_gas;
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
// TODO(AvivG): Remove allow once used.
#[allow(unused)]
pub(crate) trait EstimateCasmHashResources {
    /// Specifies the hash function variant that the estimate is for.
    fn hash_version(&self) -> HashVersion;

    /// Estimates the Cairo execution resources used when applying the hash function during CASM
    /// hashing.
    fn estimated_resources_of_hash_function(
        _felt_size_groups: &FeltSizeCount,
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

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
    fn hash_version(&self) -> HashVersion {
        HashVersion::V1
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

// TODO(AvivG): Remove allow once used.
#[allow(unused)]
pub(crate) struct CasmV2HashResourceEstimate {}

impl EstimateCasmHashResources for CasmV2HashResourceEstimate {
    fn hash_version(&self) -> HashVersion {
        HashVersion::V2
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
        let n_steps =
            Self::estimate_steps_of_encode_felt252_data_and_calc_blake_hash(felt_size_groups);
        let builtin_instance_counter = match felt_size_groups.n_felts() {
            // The empty case does not use builtins at all.
            0 => HashMap::new(),
            // One `range_check` per input felt to validate its size + Overhead for the non empty
            // case.
            _ => HashMap::from([(
                BuiltinName::range_check,
                felt_size_groups.n_felts() + Self::BASE_RANGE_CHECK_NON_EMPTY,
            )]),
        };

        let resources = ExecutionResources { n_steps, n_memory_holes: 0, builtin_instance_counter };

        EstimatedExecutionResources::V2Hash {
            resources,
            blake_count: felt_size_groups.blake_opcode_count(),
        }
    }
}

impl CasmV2HashResourceEstimate {
    // Constants used for estimating the VM execution resources of BLAKE hashing in the Starknet OS.
    // Values were obtained empirically by running
    // `encode_felt252_data_and_calc_blake_hash` on various combinations of large and small felts.

    // Per-felt contribution.
    pub const STEPS_PER_LARGE_FELT: usize = 45;
    pub const STEPS_PER_SMALL_FELT: usize = 15;

    // One-time overheads for `encode_felt252_data_and_calc_blake_hash` execution.
    // Applied when the input fills an exact Blake message (16-u32).
    pub const BASE_STEPS_FULL_MSG: usize = 217;
    // Applied when the input leaves a remainder (< 16 u32s).
    pub const BASE_STEPS_PARTIAL_MSG: usize = 195;
    // Extra steps added per 2-u32 remainder in partial messages.
    pub const STEPS_PER_2_U32_REMINDER: usize = 3;
    // Additional `range_check` instances required when the input is non-empty.
    pub const BASE_RANGE_CHECK_NON_EMPTY: usize = 3;

    // Applied when the input is completely empty.
    pub const STEPS_EMPTY_INPUT: usize = 170;

    /// Estimates the total number of VM steps needed to hash the given felts with Blake in the
    /// Starknet OS.
    fn estimate_steps_of_encode_felt252_data_and_calc_blake_hash(
        felt_size_groups: &FeltSizeCount,
    ) -> usize {
        let encoded_u32_len = felt_size_groups.encoded_u32_len();
        if encoded_u32_len == 0 {
            // The empty input case is a special case.
            return Self::STEPS_EMPTY_INPUT;
        }

        // Adds a base cost depending on whether the total fits exactly into full 16-u32 messages.
        let base_steps = if encoded_u32_len % FeltSizeCount::U32_WORDS_PER_MESSAGE == 0 {
            Self::BASE_STEPS_FULL_MSG
        } else {
            // This computation is based on running blake2s with different inputs.
            // Note: all inputs expand to an even number of u32s --> `rem_u32s` is always even.
            Self::BASE_STEPS_PARTIAL_MSG
                + (encoded_u32_len % FeltSizeCount::U32_WORDS_PER_MESSAGE / 2)
                    * Self::STEPS_PER_2_U32_REMINDER
        };

        base_steps
            + felt_size_groups.large * Self::STEPS_PER_LARGE_FELT
            + felt_size_groups.small * Self::STEPS_PER_SMALL_FELT
    }

    /// Converts the execution resources and blake opcode count to L2 gas.
    ///
    /// Used for both Stwo ("proving_gas") and Stone ("sierra_gas") estimations, which differ in
    /// builtin costs. This unified logic is valid because only the `range_check` builtin is used,
    /// and its cost is identical across provers (see `bouncer.get_tx_weights`).
    // TODO(AvivG): Move inside blake estimation struct.
    pub(crate) fn blake_execution_resources_estimation_to_gas(
        resources: EstimatedExecutionResources,
        versioned_constants: &VersionedConstants,
        blake_opcode_gas: usize,
    ) -> GasAmount {
        // TODO(AvivG): Remove this once gas computation is separated from resource estimation.
        assert!(
            resources
                .resources()
                .builtin_instance_counter
                .keys()
                .all(|&k| k == BuiltinName::range_check),
            "Expected either empty builtins or only `range_check` builtin, got: {:?}. This breaks \
             the assumption that builtin costs are identical between provers.",
            resources.resources().builtin_instance_counter.keys().collect::<Vec<_>>()
        );

        resources.to_sierra_gas(
            |resources| vm_resources_to_sierra_gas(resources, versioned_constants),
            Some(blake_opcode_gas),
        )
    }
}
