use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::NestedIntList;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use itertools::Itertools;
use starknet_api::contract_class::{
    ClassInfo,
    ContractClass,
    ContractClassV0,
    ContractClassV0Inner,
    ContractClassV1,
    ContractClassV1Inner,
    EntryPointV1,
};
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::EntryPointType;

use super::execution_utils::poseidon_hash_many_cost;
use crate::abi::abi_utils::selector_from_name;
use crate::abi::constants::{self, CONSTRUCTOR_ENTRY_POINT_NAME};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::{ContractClassError, PreExecutionError};
use crate::fee::eth_gas_constants;
use crate::transaction::errors::TransactionExecutionError;

#[cfg(test)]
#[path = "contract_class_test.rs"]
pub mod test;

/// Represents a runnable Starknet contract class (meaning, the program is runnable by the VM).
/// We wrap the actual class in an Arc to avoid cloning the program when cloning the class.
// Note: when deserializing from a SN API class JSON string, the ABI field is ignored
// by serde, since it is not required for execution.

pub type ContractClassResult<T> = Result<T, ContractClassError>;

pub trait ContractClassExt {
    fn constructor_selector(&self) -> Option<EntryPointSelector>;
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources;
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError>;
    fn bytecode_length(&self) -> usize;
}

impl ContractClassExt for ContractClass {
    fn constructor_selector(&self) -> Option<EntryPointSelector> {
        match self {
            ContractClass::V0(class) => class.constructor_selector(),
            ContractClass::V1(class) => class.constructor_selector(),
        }
    }

    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        match self {
            ContractClass::V0(class) => class.estimate_casm_hash_computation_resources(),
            ContractClass::V1(class) => class.estimate_casm_hash_computation_resources(),
        }
    }

    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError> {
        match self {
            ContractClass::V0(_) => {
                panic!("get_visited_segments is not supported for v0 contracts.")
            }
            ContractClass::V1(class) => class.get_visited_segments(visited_pcs),
        }
    }

    fn bytecode_length(&self) -> usize {
        match self {
            ContractClass::V0(class) => class.bytecode_length(),
            ContractClass::V1(class) => class.bytecode_length(),
        }
    }
}

// V0.
trait ContractClassV0Ext {
    fn constructor_selector(&self) -> Option<EntryPointSelector>;
    fn n_entry_points(&self) -> usize;
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources;
}

impl ContractClassV0Ext for ContractClassV0 {
    fn constructor_selector(&self) -> Option<EntryPointSelector> {
        Some(self.entry_points_by_type[&EntryPointType::Constructor].first()?.selector)
    }

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

pub trait ContractClassV0PubExt {
    fn n_builtins(&self) -> usize;
    fn bytecode_length(&self) -> usize;
    fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV0, ProgramError>;
}

impl ContractClassV0PubExt for ContractClassV0 {
    fn n_builtins(&self) -> usize {
        self.program.builtins_len()
    }

    fn bytecode_length(&self) -> usize {
        self.program.data_len()
    }

    fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV0, ProgramError> {
        let contract_class: ContractClassV0Inner = serde_json::from_str(raw_contract_class)?;
        Ok(ContractClassV0(Arc::new(contract_class)))
    }
}

// V1.
trait ContractClassV1Ext {
    fn constructor_selector(&self) -> Option<EntryPointSelector>;
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources;
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError>;
}

pub trait ContractClassV1PubExt {
    fn bytecode_length(&self) -> usize;
    fn bytecode_segment_lengths(&self) -> &NestedIntList;
    fn get_entry_point(&self, call: &CallEntryPoint) -> Result<EntryPointV1, PreExecutionError>;
    fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV1, ProgramError>;
    #[cfg(any(feature = "testing", test))]
    fn empty_for_testing() -> Self;
}

impl ContractClassV1Ext for ContractClassV1 {
    fn constructor_selector(&self) -> Option<EntryPointSelector> {
        Some(self.0.entry_points_by_type[&EntryPointType::Constructor].first()?.selector)
    }

    /// Returns the estimated VM resources required for computing Casm hash.
    /// This is an empiric measurement of several bytecode lengths, which constitutes as the
    /// dominant factor in it.
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        estimate_casm_hash_computation_resources(&self.bytecode_segment_lengths)
    }

    // Returns the set of segments that were visited according to the given visited PCs.
    // Each visited segment must have its starting PC visited, and is represented by it.
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError> {
        let mut reversed_visited_pcs: Vec<_> = visited_pcs.iter().cloned().sorted().rev().collect();
        get_visited_segments(&self.bytecode_segment_lengths, &mut reversed_visited_pcs, &mut 0)
    }
}

impl ContractClassV1PubExt for ContractClassV1 {
    fn bytecode_length(&self) -> usize {
        self.program.data_len()
    }

    fn bytecode_segment_lengths(&self) -> &NestedIntList {
        &self.bytecode_segment_lengths
    }

    fn get_entry_point(&self, call: &CallEntryPoint) -> Result<EntryPointV1, PreExecutionError> {
        if call.entry_point_type == EntryPointType::Constructor
            && call.entry_point_selector != selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME)
        {
            return Err(PreExecutionError::InvalidConstructorEntryPointName);
        }

        let entry_points_of_same_type = &self.0.entry_points_by_type[&call.entry_point_type];
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

    fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV1, ProgramError> {
        let casm_contract_class: CasmContractClass = serde_json::from_str(raw_contract_class)?;
        let contract_class = casm_contract_class.try_into()?;

        Ok(contract_class)
    }

    /// Returns an empty contract class for testing purposes.
    #[cfg(any(feature = "testing", test))]
    fn empty_for_testing() -> Self {
        Self(Arc::new(ContractClassV1Inner {
            program: Default::default(),
            entry_points_by_type: Default::default(),
            hints: Default::default(),
            bytecode_segment_lengths: NestedIntList::Leaf(0),
        }))
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
fn get_visited_segments(
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
                    get_visited_segments(segment, visited_pcs, bytecode_offset)?;

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

pub trait ClassInfoExt: Sized {
    fn bytecode_length(&self) -> usize;
    fn contract_class(&self) -> ContractClass;
    fn sierra_program_length(&self) -> usize;
    fn abi_length(&self) -> usize;
    fn code_size(&self) -> usize;
    fn new(
        contract_class: &ContractClass,
        sierra_program_length: usize,
        abi_length: usize,
    ) -> ContractClassResult<Self>;
}

impl ClassInfoExt for ClassInfo {
    fn bytecode_length(&self) -> usize {
        self.contract_class.bytecode_length()
    }

    fn contract_class(&self) -> ContractClass {
        self.contract_class.clone()
    }

    fn sierra_program_length(&self) -> usize {
        self.sierra_program_length
    }

    fn abi_length(&self) -> usize {
        self.abi_length
    }

    fn code_size(&self) -> usize {
        (self.bytecode_length() + self.sierra_program_length())
            // We assume each felt is a word.
            * eth_gas_constants::WORD_WIDTH
            + self.abi_length()
    }

    fn new(
        contract_class: &ContractClass,
        sierra_program_length: usize,
        abi_length: usize,
    ) -> ContractClassResult<Self> {
        let (contract_class_version, condition) = match contract_class {
            ContractClass::V0(_) => (0, sierra_program_length == 0),
            ContractClass::V1(_) => (1, sierra_program_length > 0),
        };

        if condition {
            Ok(Self { contract_class: contract_class.clone(), sierra_program_length, abi_length })
        } else {
            Err(ContractClassError::ContractClassVersionSierraProgramLengthMismatch {
                contract_class_version,
                sierra_program_length,
            })
        }
    }
}
