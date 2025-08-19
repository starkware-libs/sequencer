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
    #[cfg(test)]
    fn blake_count(&self) -> usize {
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
pub trait EstimateCasmHashResources {
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
struct CasmV1HashResourceEstimate {}

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

pub struct CasmV2HashResourceEstimate {}

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
            blake_count: felt_size_groups.blake_opcode_count(),
        }
    }
}

// Constants used for estimating the cost of BLAKE hashing inside Starknet OS.
// These values are based on empirical measurement by running
// `encode_felt252_data_and_calc_blake_hash` on various combinations of big and small felts.
mod blake_estimation {
    // Per-felt step cost (measured).
    pub const STEPS_BIG_FELT: usize = 45;
    pub const STEPS_SMALL_FELT: usize = 15;

    // One-time overhead.
    // Overhead when input fills a full Blake message (16 u32s).
    pub const BASE_STEPS_FULL_MSG: usize = 217;
    // Overhead when input results in a partial message (remainder < 16 u32s).
    pub const BASE_STEPS_PARTIAL_MSG: usize = 195;
    // Extra steps per 2-u32 remainder in partial messages.
    pub const STEPS_PER_2_U32_REMINDER: usize = 3;
    // Overhead when input for `encode_felt252_data_and_calc_blake_hash` is non-empty.
    pub const BASE_RANGE_CHECK_NON_EMPTY: usize = 3;
    // Empty input steps.
    pub const STEPS_EMPTY_INPUT: usize = 170;
}

fn base_steps_for_blake_hash(n_u32s: usize) -> usize {
    let rem_u32s = n_u32s % FeltSizeCount::U32_WORDS_PER_MESSAGE;
    if rem_u32s == 0 {
        blake_estimation::BASE_STEPS_FULL_MSG
    } else {
        // This computation is based on running blake2s with different inputs.
        // Note: all inputs expand to an even number of u32s --> `rem_u32s` is always even.
        blake_estimation::BASE_STEPS_PARTIAL_MSG
            + (rem_u32s / 2) * blake_estimation::STEPS_PER_2_U32_REMINDER
    }
}

fn felts_steps(n_big_felts: usize, n_small_felts: usize) -> usize {
    let big_steps = n_big_felts
        .checked_mul(blake_estimation::STEPS_BIG_FELT)
        .expect("Overflow computing big felt steps");
    let small_steps = n_small_felts
        .checked_mul(blake_estimation::STEPS_SMALL_FELT)
        .expect("Overflow computing small felt steps");
    big_steps.checked_add(small_steps).expect("Overflow computing total felt steps")
}

/// Estimates the number of VM steps needed to hash the given felts with Blake in Starknet OS.
/// Each small felt unpacks into 2 u32s, and each big felt into 8 u32s.
/// Adds a base cost depending on whether the total fits exactly into full 16-u32 messages.
fn estimate_steps_of_encode_felt252_data_and_calc_blake_hash(
    felt_size_groups: &FeltSizeCount,
) -> usize {
    let total_u32s = felt_size_groups.encoded_u32_len();
    if total_u32s == 0 {
        // The empty input case is a special case.
        return blake_estimation::STEPS_EMPTY_INPUT;
    }

    let base_steps = base_steps_for_blake_hash(total_u32s);
    let felt_steps = felts_steps(felt_size_groups.large, felt_size_groups.small);

    base_steps.checked_add(felt_steps).expect("Overflow computing total Blake hash steps")
}

/// Converts the execution resources and blake opcode count to L2 gas.
///
/// Used for both Stwo ("proving_gas") and Stone ("sierra_gas") estimations, which differ in
/// builtin costs. This unified logic is valid because only the `range_check` builtin is used,
/// and its cost is identical across provers (see `bouncer.get_tx_weights`).
// TODO(AvivG): Move inside blake estimation struct.
pub fn blake_execution_resources_estimation_to_gas(
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
        "Expected either empty builtins or only `range_check` builtin, got: {:?}. This breaks the \
         assumption that builtin costs are identical between provers.",
        resources.resources().builtin_instance_counter.keys().collect::<Vec<_>>()
    );

    resources.to_sierra_gas(
        |resources| vm_resources_to_sierra_gas(resources, versioned_constants),
        Some(blake_opcode_gas),
    )
}
