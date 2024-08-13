use std::collections::{HashMap, HashSet};

use cairo_lang_starknet_classes::NestedIntList;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use itertools::Itertools;
use starknet_api::contract_class::{
    ClassInfo,
    ContractClass,
    ContractClassV0,
    ContractClassV1,
    EntryPointV1,
};
use starknet_api::deprecated_contract_class::EntryPointType;

use super::execution_utils::poseidon_hash_many_cost;
use crate::abi::abi_utils::selector_from_name;
use crate::abi::constants::{self, CONSTRUCTOR_ENTRY_POINT_NAME};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::PreExecutionError;
use crate::fee::eth_gas_constants;
use crate::transaction::errors::TransactionExecutionError;

#[cfg(test)]
#[path = "contract_class_test.rs"]
pub mod test;

/// Represents a runnable Starknet contract class (meaning, the program is runnable by the VM).
/// We wrap the actual class in an Arc to avoid cloning the program when cloning the class.
// Note: when deserializing from a SN API class JSON string, the ABI field is ignored
// by serde, since it is not required for execution.

pub fn estimate_casm_hash_computation_resources_from_contract_class(
    contract_class: &ContractClass,
) -> ExecutionResources {
    match contract_class {
        ContractClass::V0(class) => class.estimate_casm_hash_computation_resources(),
        ContractClass::V1(class) => class.estimate_casm_hash_computation_resources(),
    }
}

pub fn get_visited_segments(
    contract_class: &ContractClass,
    visited_pcs: &HashSet<usize>,
) -> Result<Vec<usize>, TransactionExecutionError> {
    match contract_class {
        ContractClass::V0(_) => {
            panic!("get_visited_segments is not supported for v0 contracts.")
        }
        ContractClass::V1(class) => class.get_visited_segments(visited_pcs),
    }
}

// V0.
trait ContractClassV0Ext {
    fn n_entry_points(&self) -> usize;
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources;
}

impl ContractClassV0Ext for ContractClassV0 {
    fn n_entry_points(&self) -> usize {
        self.entry_points_by_type.values().map(|vec| vec.len()).sum()
    }

    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        let hashed_data_size = (constants::CAIRO0_ENTRY_POINT_STRUCT_SIZE * self.n_entry_points())
            + self.n_builtins()
            + self.bytecode_length()
            + 1; // Hinted class hash.
        // The hashed data size is approximately the number of hashes (invoked in hash chains).
        let n_steps = constants::N_STEPS_PER_PEDERSEN * hashed_data_size;

        ExecutionResources {
            n_steps,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::pedersen, hashed_data_size)]),
        }
    }
}

// V1.
trait ContractClassV1Ext {
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources;
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError>;
}

impl ContractClassV1Ext for ContractClassV1 {
    /// Returns the estimated VM resources required for computing Casm hash.
    /// This is an empiric measurement of several bytecode lengths, which constitutes as the
    /// dominant factor in it.
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        estimate_casm_hash_computation_resources(self.bytecode_segment_lengths())
    }

    // Returns the set of segments that were visited according to the given visited PCs.
    // Each visited segment must have its starting PC visited, and is represented by it.
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError> {
        let mut reversed_visited_pcs: Vec<_> = visited_pcs.iter().cloned().sorted().rev().collect();
        internal_get_visited_segments(
            self.bytecode_segment_lengths(),
            &mut reversed_visited_pcs,
            &mut 0,
        )
    }
}

pub fn get_entry_point(
    contract_class: &ContractClassV1,
    call: &CallEntryPoint,
) -> Result<EntryPointV1, PreExecutionError> {
    if call.entry_point_type == EntryPointType::Constructor
        && call.entry_point_selector != selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME)
    {
        return Err(PreExecutionError::InvalidConstructorEntryPointName);
    }

    let entry_points_of_same_type = &contract_class.0.entry_points_by_type[&call.entry_point_type];
    let filtered_entry_points: Vec<_> = entry_points_of_same_type
        .iter()
        .filter(|ep| ep.selector == call.entry_point_selector)
        .collect();

    match &filtered_entry_points[..] {
        [] => Err(PreExecutionError::EntryPointNotFound(call.entry_point_selector)),
        [entry_point] => Ok((*entry_point).clone()),
        _ => Err(PreExecutionError::DuplicatedEntryPointSelector {
            selector: call.entry_point_selector,
            typ: call.entry_point_type,
        }),
    }
}

/// Returns the estimated VM resources required for computing Casm hash (for Cairo 1 contracts).
///
/// Note: the function focuses on the bytecode size, and currently ignores the cost handling the
/// class entry points.
pub fn estimate_casm_hash_computation_resources(
    bytecode_segment_lengths: &NestedIntList,
) -> ExecutionResources {
    // The constants in this function were computed by running the Casm code on a few values
    // of `bytecode_segment_lengths`.
    match bytecode_segment_lengths {
        NestedIntList::Leaf(length) => {
            // The entire contract is a single segment (old Sierra contracts).
            &ExecutionResources {
                n_steps: 474,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 10)]),
            } + &poseidon_hash_many_cost(*length)
        }
        NestedIntList::Node(segments) => {
            // The contract code is segmented by its functions.
            let mut execution_resources = ExecutionResources {
                n_steps: 491,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 11)]),
            };
            let base_segment_cost = ExecutionResources {
                n_steps: 24,
                n_memory_holes: 1,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 1)]),
            };
            for segment in segments {
                let NestedIntList::Leaf(length) = segment else {
                    panic!(
                        "Estimating hash cost is only supported for segmentation depth at most 1."
                    );
                };
                execution_resources += &poseidon_hash_many_cost(*length);
                execution_resources += &base_segment_cost;
            }
            execution_resources
        }
    }
}

// Returns the set of segments that were visited according to the given visited PCs and segment
// lengths.
// Each visited segment must have its starting PC visited, and is represented by it.
// visited_pcs should be given in reversed order, and is consumed by the function.
fn internal_get_visited_segments(
    segment_lengths: &NestedIntList,
    visited_pcs: &mut Vec<usize>,
    bytecode_offset: &mut usize,
) -> Result<Vec<usize>, TransactionExecutionError> {
    let mut res = Vec::new();

    match segment_lengths {
        NestedIntList::Leaf(length) => {
            let segment = *bytecode_offset..*bytecode_offset + length;
            if visited_pcs.last().is_some_and(|pc| segment.contains(pc)) {
                res.push(segment.start);
            }

            while visited_pcs.last().is_some_and(|pc| segment.contains(pc)) {
                visited_pcs.pop();
            }
            *bytecode_offset += length;
        }
        NestedIntList::Node(segments) => {
            for segment in segments {
                let segment_start = *bytecode_offset;
                let next_visited_pc = visited_pcs.last().copied();

                let visited_inner_segments =
                    internal_get_visited_segments(segment, visited_pcs, bytecode_offset)?;

                if next_visited_pc.is_some_and(|pc| pc != segment_start)
                    && !visited_inner_segments.is_empty()
                {
                    return Err(TransactionExecutionError::InvalidSegmentStructure(
                        next_visited_pc.unwrap(),
                        segment_start,
                    ));
                }

                res.extend(visited_inner_segments);
            }
        }
    }
    Ok(res)
}

pub fn get_code_size(class_info: &ClassInfo) -> usize {
    (class_info.bytecode_length() + class_info.sierra_program_length())
    // We assume each felt is a word.
    * eth_gas_constants::WORD_WIDTH
        + class_info.abi_length()
}
