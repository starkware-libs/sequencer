use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::sync::Arc;

use cairo_lang_casm;
use cairo_lang_casm::hints::Hint;
use cairo_lang_starknet_classes::casm_contract_class::{CasmContractClass, CasmContractEntryPoint};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_vm::serde::deserialize_program::{
    ApTracking,
    FlowTrackingData,
    HintParams,
    ReferenceManager,
};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use itertools::Itertools;
use semver::Version;
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPoint,
    EntryPointOffset,
    EntryPointType,
    Program as DeprecatedProgram,
};
use starknet_types_core::felt::Felt;

use super::execution_utils::poseidon_hash_many_cost;
use crate::abi::abi_utils::selector_from_name;
use crate::abi::constants::{self, CONSTRUCTOR_ENTRY_POINT_NAME};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::{ContractClassError, PreExecutionError};
use crate::execution::execution_utils::sn_api_to_cairo_vm_program;
use crate::fee::eth_gas_constants;
use crate::transaction::errors::TransactionExecutionError;
use crate::versioned_constants::CompilerVersion;

#[cfg(test)]
#[path = "contract_class_test.rs"]
pub mod test;

pub type ContractClassResult<T> = Result<T, ContractClassError>;

/// The resource used to run a contract function.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Serialize)]
pub enum TrackingResource {
    #[default]
    CairoSteps, // AKA VM mode.
    SierraGas, // AKA Sierra mode.
}

/// Represents a runnable Starknet contract class (meaning, the program is runnable by the VM).
#[derive(Clone, Debug, Eq, PartialEq, derive_more::From)]
pub enum ContractClass {
    V0(ContractClassV0),
    V1(ContractClassV1),
}

impl TryFrom<&CasmContractClass> for ContractClass {
    type Error = ProgramError;

    fn try_from(contract_class: &CasmContractClass) -> Result<Self, Self::Error> {
        Ok(ContractClass::V1(contract_class.try_into()?))
    }
}

impl ContractClass {
    pub fn constructor_selector(&self) -> Option<EntryPointSelector> {
        match self {
            ContractClass::V0(class) => class.constructor_selector(),
            ContractClass::V1(class) => class.constructor_selector(),
        }
    }

    pub fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        match self {
            ContractClass::V0(class) => class.estimate_casm_hash_computation_resources(),
            ContractClass::V1(class) => class.estimate_casm_hash_computation_resources(),
        }
    }

    pub fn get_visited_segments(
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

    pub fn bytecode_length(&self) -> usize {
        match self {
            ContractClass::V0(class) => class.bytecode_length(),
            ContractClass::V1(class) => class.bytecode_length(),
        }
    }

    /// Returns whether this contract should run using Cairo steps or Sierra gas.
    pub fn tracking_resource(&self, min_sierra_version: &CompilerVersion) -> TrackingResource {
        match self {
            ContractClass::V0(_) => TrackingResource::CairoSteps,
            ContractClass::V1(contract_class) => {
                contract_class.tracking_resource(min_sierra_version)
            }
        }
    }
}

// V0.

/// Represents a runnable Cairo 0 Starknet contract class (meaning, the program is runnable by the
/// VM). We wrap the actual class in an Arc to avoid cloning the program when cloning the
/// class.
// Note: when deserializing from a SN API class JSON string, the ABI field is ignored
// by serde, since it is not required for execution.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ContractClassV0(pub Arc<ContractClassV0Inner>);
impl Deref for ContractClassV0 {
    type Target = ContractClassV0Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ContractClassV0 {
    fn constructor_selector(&self) -> Option<EntryPointSelector> {
        Some(self.entry_points_by_type[&EntryPointType::Constructor].first()?.selector)
    }

    fn n_entry_points(&self) -> usize {
        self.entry_points_by_type.values().map(|vec| vec.len()).sum()
    }

    pub fn n_builtins(&self) -> usize {
        self.program.builtins_len()
    }

    pub fn bytecode_length(&self) -> usize {
        self.program.data_len()
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

    pub fn tracking_resource(&self) -> TrackingResource {
        TrackingResource::CairoSteps
    }

    pub fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV0, ProgramError> {
        let contract_class: ContractClassV0Inner = serde_json::from_str(raw_contract_class)?;
        Ok(ContractClassV0(Arc::new(contract_class)))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct ContractClassV0Inner {
    #[serde(deserialize_with = "deserialize_program")]
    pub program: Program,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>,
}

impl TryFrom<DeprecatedContractClass> for ContractClassV0 {
    type Error = ProgramError;

    fn try_from(class: DeprecatedContractClass) -> Result<Self, Self::Error> {
        Ok(Self(Arc::new(ContractClassV0Inner {
            program: sn_api_to_cairo_vm_program(class.program)?,
            entry_points_by_type: class.entry_points_by_type,
        })))
    }
}

// V1.

/// Represents a runnable Cario (Cairo 1) Starknet contract class (meaning, the program is runnable
/// by the VM). We wrap the actual class in an Arc to avoid cloning the program when cloning the
/// class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractClassV1(pub Arc<ContractClassV1Inner>);
impl Deref for ContractClassV1 {
    type Target = ContractClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ContractClassV1 {
    fn constructor_selector(&self) -> Option<EntryPointSelector> {
        Some(self.0.entry_points_by_type[&EntryPointType::Constructor].first()?.selector)
    }

    pub fn bytecode_length(&self) -> usize {
        self.program.data_len()
    }

    pub fn bytecode_segment_lengths(&self) -> &NestedIntList {
        &self.bytecode_segment_lengths
    }

    pub fn get_entry_point(
        &self,
        call: &CallEntryPoint,
    ) -> Result<EntryPointV1, PreExecutionError> {
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

    /// Returns whether this contract should run using Cairo steps or Sierra gas.
    pub fn tracking_resource(&self, min_sierra_version: &CompilerVersion) -> TrackingResource {
        if *min_sierra_version <= self.compiler_version {
            TrackingResource::SierraGas
        } else {
            TrackingResource::CairoSteps
        }
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

    pub fn try_from_json_string(raw_contract_class: &str) -> Result<ContractClassV1, ProgramError> {
        let casm_contract_class: CasmContractClass = serde_json::from_str(raw_contract_class)?;
        let contract_class: ContractClassV1 = (&casm_contract_class).try_into()?;

        Ok(contract_class)
    }

    /// Returns an empty contract class for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn empty_for_testing() -> Self {
        Self(Arc::new(ContractClassV1Inner {
            program: Default::default(),
            entry_points_by_type: Default::default(),
            hints: Default::default(),
            compiler_version: Default::default(),
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
                n_steps: 463,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 10)]),
            } + &poseidon_hash_many_cost(*length)
        }
        NestedIntList::Node(segments) => {
            // The contract code is segmented by its functions.
            let mut execution_resources = ExecutionResources {
                n_steps: 480,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractClassV1Inner {
    pub program: Program,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPointV1>>,
    pub hints: HashMap<String, Hint>,
    pub compiler_version: CompilerVersion,
    bytecode_segment_lengths: NestedIntList,
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EntryPointV1 {
    pub selector: EntryPointSelector,
    pub offset: EntryPointOffset,
    pub builtins: Vec<BuiltinName>,
}

impl EntryPointV1 {
    pub fn pc(&self) -> usize {
        self.offset.0
    }
}

impl TryFrom<&CasmContractClass> for ContractClassV1 {
    type Error = ProgramError;

    fn try_from(class: &CasmContractClass) -> Result<Self, Self::Error> {
        let data: Vec<MaybeRelocatable> =
            class.bytecode.iter().map(|x| MaybeRelocatable::from(Felt::from(&x.value))).collect();

        let mut hints: HashMap<usize, Vec<HintParams>> = HashMap::new();
        for (i, hint_list) in class.hints.iter() {
            let hint_params: Result<Vec<HintParams>, ProgramError> =
                hint_list.iter().map(hint_to_hint_params).collect();
            hints.insert(*i, hint_params?);
        }

        // Collect a sting to hint map so that the hint processor can fetch the correct [Hint]
        // for each instruction.
        let mut string_to_hint: HashMap<String, Hint> = HashMap::new();
        for (_, hint_list) in class.hints.iter() {
            for hint in hint_list.iter() {
                string_to_hint.insert(serde_json::to_string(hint)?, hint.clone());
            }
        }

        let builtins = vec![]; // The builtins are initialize later.
        let main = Some(0);
        let reference_manager = ReferenceManager { references: Vec::new() };
        let identifiers = HashMap::new();
        let error_message_attributes = vec![];
        let instruction_locations = None;

        let program = Program::new(
            builtins,
            data,
            main,
            hints,
            reference_manager,
            identifiers,
            error_message_attributes,
            instruction_locations,
        )?;

        let mut entry_points_by_type = HashMap::new();
        entry_points_by_type.insert(
            EntryPointType::Constructor,
            convert_entry_points_v1(&class.entry_points_by_type.constructor),
        );
        entry_points_by_type.insert(
            EntryPointType::External,
            convert_entry_points_v1(&class.entry_points_by_type.external),
        );
        entry_points_by_type.insert(
            EntryPointType::L1Handler,
            convert_entry_points_v1(&class.entry_points_by_type.l1_handler),
        );

        let bytecode_segment_lengths = class
            .bytecode_segment_lengths
            .clone()
            .unwrap_or_else(|| NestedIntList::Leaf(program.data_len()));
        let compiler_version = CompilerVersion(
            Version::parse(&class.compiler_version)
                .unwrap_or_else(|_| panic!("Invalid version: '{}'", class.compiler_version)),
        );
        Ok(Self(Arc::new(ContractClassV1Inner {
            program,
            entry_points_by_type,
            hints: string_to_hint,
            compiler_version,
            bytecode_segment_lengths,
        })))
    }
}

// V0 utilities.

/// Converts the program type from SN API into a Cairo VM-compatible type.
pub fn deserialize_program<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Program, D::Error> {
    let deprecated_program = DeprecatedProgram::deserialize(deserializer)?;
    sn_api_to_cairo_vm_program(deprecated_program)
        .map_err(|err| DeserializationError::custom(err.to_string()))
}

// V1 utilities.

// TODO(spapini): Share with cairo-lang-runner.
fn hint_to_hint_params(hint: &cairo_lang_casm::hints::Hint) -> Result<HintParams, ProgramError> {
    Ok(HintParams {
        code: serde_json::to_string(hint)?,
        accessible_scopes: vec![],
        flow_tracking_data: FlowTrackingData {
            ap_tracking: ApTracking::new(),
            reference_ids: HashMap::new(),
        },
    })
}

fn convert_entry_points_v1(external: &[CasmContractEntryPoint]) -> Vec<EntryPointV1> {
    external
        .iter()
        .map(|ep| EntryPointV1 {
            selector: EntryPointSelector(Felt::from(&ep.selector)),
            offset: EntryPointOffset(ep.offset),
            builtins: ep
                .builtins
                .iter()
                .map(|builtin| BuiltinName::from_str(builtin).expect("Unrecognized builtin."))
                .collect(),
        })
        .collect()
}

#[derive(Clone, Debug)]
// TODO(Ayelet,10/02/2024): Change to bytes.
pub struct ClassInfo {
    contract_class: ContractClass,
    sierra_program_length: usize,
    abi_length: usize,
}

impl TryFrom<&starknet_api::contract_class::ClassInfo> for ClassInfo {
    type Error = ProgramError;

    fn try_from(class_info: &starknet_api::contract_class::ClassInfo) -> Result<Self, Self::Error> {
        let starknet_api::contract_class::ClassInfo {
            casm_contract_class,
            sierra_program_length,
            abi_length,
        } = class_info;

        Ok(Self {
            contract_class: casm_contract_class.try_into()?,
            sierra_program_length: *sierra_program_length,
            abi_length: *abi_length,
        })
    }
}

impl ClassInfo {
    pub fn bytecode_length(&self) -> usize {
        self.contract_class.bytecode_length()
    }

    pub fn contract_class(&self) -> ContractClass {
        self.contract_class.clone()
    }

    pub fn sierra_program_length(&self) -> usize {
        self.sierra_program_length
    }

    pub fn abi_length(&self) -> usize {
        self.abi_length
    }

    pub fn code_size(&self) -> usize {
        (self.bytecode_length() + self.sierra_program_length())
            // We assume each felt is a word.
            * eth_gas_constants::WORD_WIDTH
            + self.abi_length()
    }

    pub fn new(
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
