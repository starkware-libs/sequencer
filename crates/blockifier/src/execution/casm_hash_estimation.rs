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

    pub fn mut_resources(&mut self) -> &mut ExecutionResources {
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
    fn from_resources(resources: ExecutionResources) -> EstimatedExecutionResources;

    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources;

    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources used when applying the hash function during CASM
    /// hashing.
    fn estimated_resources_of_hash_function(
        _felt_size_groups: &FeltSizeCount,
    ) -> EstimatedExecutionResources;

    /// Estimates the Cairo execution resources for `compiled_class_hash` in the
    /// Starknet OS.
    fn estimated_resources_of_compiled_class_hash(
        bytecode_segment_felt_sizes: &NestedFeltCounts,
        entry_points_by_type: &EntryPointsByType<EntryPointV1>,
    ) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_COMPILED_CLASS_HASH,
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
        // hash_ep2, hash_ep3, hash_bytecode)
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
            n_steps: cairo_functions_step_estimation::BASE_BYTECODE_HASH_NODE,
            ..Default::default()
        });

        // Add leaf vs node cost
        resources += &match bytecode_segment_felt_sizes {
            // Single-segment contract (e.g., older Sierra contracts).
            NestedFeltCounts::Leaf(_, felt_size_groups) => {
                Self::estimated_resources_of_hash_function(felt_size_groups)
            }
            NestedFeltCounts::Node(segments) => Self::node_cost(segments),
        };

        resources
    }

    fn estimated_resources_of_hash_entry_points(
        entry_points: &[EntryPointV1],
    ) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_HASH_ENTRY_POINTS,
            ..Default::default()
        });

        for entry_point in entry_points {
            resources += &Self::estimated_resources_of_hash_entry_points_inner(entry_point);
        }

        // Computes cost of `hash_finalize`: a hash over (selector1, offset1, selector2, offset2,
        // ...). Each entry point has a selector (big felt) and an offset (small felt).
        // somethis with builtins make the large *2.
        resources += &self.estimated_resources_of_hash_function(&FeltSizeCount {
            large: entry_points.len() + entry_points.len(),
            small: entry_points.len(),
        });

        resources
    }

    fn estimated_resources_of_hash_entry_points_inner(
        entry_point: &EntryPointV1,
    ) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_HASH_ENTRY_POINTS_INNER,
            ..Default::default()
        });

        // compute cost of `hash_update_with_nested_hash`
        let base_resources_of_hash_update_with_nested_hash = ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_HASH_UPDATE_WITH_NESTED_HASH,
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

pub struct CasmV1HashResourceEstimate;

impl EstimateCasmHashResources for CasmV1HashResourceEstimate {
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

    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources {
        // The entire contract is a single segment (old Sierra contracts).
        let mut resources = Self::estimated_resources_of_hash_function(felt_size_groups);
        resources += &ExecutionResources {
            n_steps: 0, // 464,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 10)]),
        };

        resources
    }

    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources {
        // The contract code is segmented by its functions.
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: 18, // 482,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 11)]),
        });
        let base_segment_cost = ExecutionResources {
            n_steps: 25,
            n_memory_holes: 1,
            builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 1)]),
        };
        for segment in bytecode_segment_felt_sizes {
            let NestedFeltCounts::Leaf(length, _) = segment else {
                panic!("Estimating hash cost is only supported for segmentation depth at most 1.");
            };
            resources += &poseidon_hash_many_cost(*length);
            resources += &base_segment_cost;
        }
        resources
    }
}

pub struct CasmV2HashResourceEstimate;

impl EstimateCasmHashResources for CasmV2HashResourceEstimate {
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
            blake_count: felt_size_groups.blake_opcode_count(),
        }
    }

    fn leaf_cost(felt_size_groups: &FeltSizeCount) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_BYTECODE_HASH_NODE_LEAF,
            ..Default::default()
        });
        resources += &Self::estimated_resources_of_hash_function(felt_size_groups);

        resources
    }

    fn node_cost(bytecode_segment_felt_sizes: &[NestedFeltCounts]) -> EstimatedExecutionResources {
        let mut resources = Self::from_resources(ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_BYTECODE_HASH_NODE_NODE,
            ..Default::default()
        });

        let bytecode_hash_internal_node_overhead = ExecutionResources {
            n_steps: cairo_functions_step_estimation::BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF,
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

        // Node‐level hash over (hash1, len1, hash2, len2, …): one segment hash (“big” felt))
        // and one segment length (“small” felt) per segment.
        resources += &Self::estimated_resources_of_hash_function(&FeltSizeCount {
            large: bytecode_segment_felt_sizes.len(),
            small: bytecode_segment_felt_sizes.len(),
        });

        resources
    }
}

impl Default for CasmV2HashResourceEstimate {
    fn default() -> Self {
        CasmV2HashResourceEstimate {}
    }
}

mod cairo_functions_step_estimation {
    // Call functions steps.
    const CALL_COMPILED_CLASS_HASH: usize = 10;
    const CALL_BYTECODE_HASH_NODE: usize = 4; //verified
    const CALL_BYTECODE_HASH_INTERNAL_NODE: usize = 7; //verified
    const CALL_HASH_FINALIZE: usize = 2; //verified almost - call +return
    // Q(AvivG): if return val is none - does it still take 1 step? no --> 2
    // Q(AvivG): if arg is pointer - does it take 1 step or number of elements? if more than 1 -->
    // change
    const CALL_HASH_ENTRY_POINTS: usize = 4; //usually 3 due to compiler optimizations. //verified
    const CALL_HASH_ENTRY_POINTS_INNER: usize = 4; //verified

    const CALL_HASH_UPDATE_SINGLE: usize = 2;
    const CALL_HASH_UPDATE_WITH_NESTED_HASH: usize = 5;

    // Cairo commands steps.
    const ALLOC_LOCAL: usize = 1; //verified
    const ASSERT: usize = 2;
    const TEMPVAR: usize = 1;
    const LET: usize = 0; // not sure
    const RETURN: usize = 1; // not sure
    const CREATE_HASH_STATE: usize = 2; // not sure
    const IF: usize = 2;

    // Fixed function total steps.
    const HASH_UPDATE_SINGLE: usize = 8; //sometimes 6 with compiler optimizations. //verified
    // CALL_HASH_UPDATE_SINGLE + ASSERT + LET + RETURN + CREATE_HASH_STATE;
    const HASH_INIT: usize = 6; // CALL (1) + LET_ALLOC(3) + RETURN CREATE_HASH_STATE (2) ; // verified

    // Base steps.
    pub(crate) const BASE_COMPILED_CLASS_HASH: usize = CALL_COMPILED_CLASS_HASH
        + HASH_INIT
        + ALLOC_LOCAL
        + 2 // assert compiled class version
        //+ ASSERT
        + RETURN
        // + CREATE_HASH_STATE
        + HASH_UPDATE_SINGLE * 2
        + CALL_HASH_ENTRY_POINTS * 3
        + CALL_BYTECODE_HASH_NODE
        + CALL_HASH_FINALIZE;

    pub(crate) const BASE_BYTECODE_HASH_NODE: usize = ALLOC_LOCAL + IF + RETURN;
    pub(crate) const BASE_BYTECODE_HASH_NODE_LEAF: usize = BASE_BYTECODE_HASH_NODE;
    pub(crate) const BASE_BYTECODE_HASH_NODE_NODE: usize =
        BASE_BYTECODE_HASH_NODE + HASH_INIT + CALL_BYTECODE_HASH_INTERNAL_NODE + CALL_HASH_FINALIZE;
    // Assuming 1 segmantation layer (inner node is a leaf).
    pub(crate) const BASE_BYTECODE_HASH_INTERNAL_NODE_LEAF: usize =
        BASE_BYTECODE_HASH_INTERNAL_NODE_EMPTY
            + ALLOC_LOCAL
            + IF
            + LET
            + TEMPVAR * 2
            + HASH_UPDATE_SINGLE + 6 // verified: first time is 8 second time is 6
            + RETURN
            + CALL_BYTECODE_HASH_INTERNAL_NODE;
    // spik those becasue this is only  if not loaded which is anyway cheaper
    // + ASSERT *3;
    pub(crate) const BASE_BYTECODE_HASH_INTERNAL_NODE_EMPTY: usize = IF + RETURN;
    const BASE_HASH_FINALIZE: usize = 0; // need ?
    pub(crate) const BASE_HASH_ENTRY_POINTS: usize =
        HASH_INIT + CALL_HASH_ENTRY_POINTS_INNER + CALL_HASH_FINALIZE + HASH_UPDATE_SINGLE + RETURN;
    pub(crate) const BASE_HASH_ENTRY_POINTS_INNER: usize = IF
        + HASH_UPDATE_SINGLE * 2
        + CALL_HASH_UPDATE_WITH_NESTED_HASH
        + CALL_HASH_ENTRY_POINTS_INNER;
    pub(crate) const BASE_HASH_UPDATE_WITH_NESTED_HASH: usize = CALL_HASH_UPDATE_SINGLE + RETURN;
}
