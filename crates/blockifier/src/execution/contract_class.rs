use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::{Deref, Index};
use std::sync::Arc;

use blake2s::SMALL_THRESHOLD;
use cairo_lang_casm;
use cairo_lang_casm::hints::Hint;
use cairo_lang_starknet_classes::casm_contract_class::{CasmContractClass, CasmContractEntryPoint};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
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
use serde::de::Error as DeserializationError;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::contract_class::compiled_class_hash::{
    EntryPointHashable,
    HashableCompiledClass,
};
use starknet_api::contract_class::{ContractClass, EntryPointType, SierraVersion, VersionedCasm};
use starknet_api::core::EntryPointSelector;
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
    EntryPointV0,
    Program as DeprecatedProgram,
};
use starknet_api::execution_resources::GasAmount;
use starknet_types_core::felt::Felt;

use crate::abi::constants::{self};
use crate::blockifier_versioned_constants::VersionedConstants;
use crate::bouncer::vm_resources_to_sierra_gas;
use crate::execution::call_info::BuiltinCounterMap;
use crate::execution::entry_point::{EntryPointExecutionContext, EntryPointTypeAndSelector};
use crate::execution::errors::PreExecutionError;
use crate::execution::execution_utils::{
    cost_of_encode_felt252_data_and_calc_blake_hash,
    poseidon_hash_many_cost,
    sn_api_to_cairo_vm_program,
};
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::transaction::errors::TransactionExecutionError;

#[cfg(test)]
#[path = "contract_class_test.rs"]
pub mod test;

pub trait HasSelector {
    fn selector(&self) -> &EntryPointSelector;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeltSizeGroups {
    // Number of felts below 2^63.
    pub small: usize,
    // Number of felts above or equal to 2^63.
    pub large: usize,
}

/// Counts felts in bytecode by size (small < 2^63, large >= 2^63).
impl From<&[BigUintAsHex]> for FeltSizeGroups {
    fn from(bytecode: &[BigUintAsHex]) -> Self {
        let (small, large) = bytecode.iter().fold((0, 0), |(small_count, large_count), x| {
            if Felt::from(&x.value) < SMALL_THRESHOLD {
                (small_count + 1, large_count)
            } else {
                (small_count, large_count + 1)
            }
        });

        FeltSizeGroups { small, large }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NestedMultipleIntList {
    Leaf(usize, FeltSizeGroups), // (leaf length, felt size groups)
    Node(Vec<NestedMultipleIntList>),
}

impl From<&NestedMultipleIntList> for NestedIntList {
    /// Converts a `NestedMultipleIntList` to a `NestedIntList` by extracting only the segment
    /// lengths. This discards the felt size group information and keeps just the structure and
    /// lengths.
    fn from(value: &NestedMultipleIntList) -> Self {
        match value {
            NestedMultipleIntList::Leaf(len, _) => NestedIntList::Leaf(*len),
            NestedMultipleIntList::Node(children) => {
                NestedIntList::Node(children.iter().map(NestedIntList::from).collect())
            }
        }
    }
}

/// The resource used to run a contract function.
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Serialize)]
pub enum TrackedResource {
    #[default]
    CairoSteps, // AKA VM mode.
    SierraGas, // AKA Sierra mode.
}

/// Represents a runnable Starknet compiled class.
/// Meaning, the program is runnable by the VM (or natively).
#[derive(Clone, Debug, Eq, PartialEq, derive_more::From)]
pub enum RunnableCompiledClass {
    V0(CompiledClassV0),
    V1(CompiledClassV1),
    #[cfg(feature = "cairo_native")]
    V1Native(NativeCompiledClassV1),
}

impl TryFrom<ContractClass> for RunnableCompiledClass {
    type Error = ProgramError;

    fn try_from(raw_contract_class: ContractClass) -> Result<Self, Self::Error> {
        let contract_class: Self = match raw_contract_class {
            ContractClass::V0(raw_contract_class) => Self::V0(raw_contract_class.try_into()?),
            ContractClass::V1(versioned_casm) => Self::V1(versioned_casm.try_into()?),
        };

        Ok(contract_class)
    }
}

impl RunnableCompiledClass {
    pub fn constructor_selector(&self) -> Option<EntryPointSelector> {
        match self {
            Self::V0(class) => class.constructor_selector(),
            Self::V1(class) => class.constructor_selector(),
            #[cfg(feature = "cairo_native")]
            Self::V1Native(class) => class.constructor_selector(),
        }
    }

    pub fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        match self {
            Self::V0(class) => class.estimate_casm_hash_computation_resources(),
            Self::V1(class) => class.estimate_casm_hash_computation_resources(),
            #[cfg(feature = "cairo_native")]
            Self::V1Native(class) => class.casm().estimate_casm_hash_computation_resources(),
        }
    }

    /// Estimate the VM gas required to migrate a CompiledClassHash from Poseidon hashing to Blake.
    pub fn estimate_compiled_class_hash_migration_resources(
        &self,
        versioned_constants: &VersionedConstants,
        blake_weight: usize,
    ) -> (GasAmount, BuiltinCounterMap) {
        match self {
            Self::V0(_) => panic!(
                "v0 contracts do not have a Compiled Class Hash and therefore shouldn't be \
                 counted for migration."
            ),
            Self::V1(class) => class.estimate_compiled_class_hash_migration_resources(
                versioned_constants,
                blake_weight,
            ),
            #[cfg(feature = "cairo_native")]
            Self::V1Native(class) => class.casm().estimate_compiled_class_hash_migration_resources(
                versioned_constants,
                blake_weight,
            ),
        }
    }

    pub fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError> {
        match self {
            Self::V0(_) => {
                panic!("get_visited_segments is not supported for v0 contracts.")
            }
            Self::V1(class) => class.get_visited_segments(visited_pcs),
            #[cfg(feature = "cairo_native")]
            Self::V1Native(_) => {
                panic!("get_visited_segments is not supported for native contracts.")
            }
        }
    }

    /// Returns whether this contract should run using Cairo steps or Sierra gas.
    pub fn tracked_resource(
        &self,
        min_sierra_version: &SierraVersion,
        last_tracked_resource: Option<&TrackedResource>,
    ) -> TrackedResource {
        let contract_tracked_resource = match self {
            Self::V0(_) => TrackedResource::CairoSteps,
            Self::V1(contract_class) => contract_class.tracked_resource(min_sierra_version),
            #[cfg(feature = "cairo_native")]
            Self::V1Native(contract_class) => {
                contract_class.casm().tracked_resource(min_sierra_version)
            }
        };
        match last_tracked_resource {
            // Once we ran with CairoSteps, we will continue to run using it for all nested calls.
            Some(TrackedResource::CairoSteps) => TrackedResource::CairoSteps,
            Some(TrackedResource::SierraGas) | None => contract_tracked_resource,
        }
    }

    /// Returns the tracked resource for calling this contract from within a context.
    pub fn get_current_tracked_resource(
        &self,
        context: &EntryPointExecutionContext,
    ) -> TrackedResource {
        self.tracked_resource(
            &context.versioned_constants().min_sierra_version_for_sierra_gas,
            context.tracked_resource_stack.last(),
        )
    }
}

// V0.

/// Represents a runnable Cairo 0 Starknet contract class (meaning, the program is runnable by the
/// VM). We wrap the actual class in an Arc to avoid cloning the program when cloning the
/// class.
// Note: when deserializing from a SN API class JSON string, the ABI field is ignored
// by serde, since it is not required for execution.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CompiledClassV0(pub Arc<CompiledClassV0Inner>);
impl Deref for CompiledClassV0 {
    type Target = CompiledClassV0Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CompiledClassV0 {
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

    pub fn tracked_resource(&self) -> TrackedResource {
        TrackedResource::CairoSteps
    }

    pub fn try_from_json_string(raw_contract_class: &str) -> Result<CompiledClassV0, ProgramError> {
        let contract_class: CompiledClassV0Inner = serde_json::from_str(raw_contract_class)?;
        Ok(CompiledClassV0(Arc::new(contract_class)))
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct CompiledClassV0Inner {
    #[serde(deserialize_with = "deserialize_program")]
    pub program: Program,
    pub entry_points_by_type: HashMap<EntryPointType, Vec<EntryPointV0>>,
}

impl TryFrom<DeprecatedContractClass> for CompiledClassV0 {
    type Error = ProgramError;

    fn try_from(class: DeprecatedContractClass) -> Result<Self, Self::Error> {
        Ok(Self(Arc::new(CompiledClassV0Inner {
            program: sn_api_to_cairo_vm_program(class.program)?,
            entry_points_by_type: class.entry_points_by_type,
        })))
    }
}

// V1.

/// Represents a runnable Cario (Cairo 1) Starknet compiled class (meaning, the program is runnable
/// by the VM). We wrap the actual class in an Arc to avoid cloning the program when cloning the
/// class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledClassV1(pub Arc<ContractClassV1Inner>);
impl Deref for CompiledClassV1 {
    type Target = ContractClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl CompiledClassV1 {
    pub fn constructor_selector(&self) -> Option<EntryPointSelector> {
        self.0.entry_points_by_type.constructor.first().map(|ep| ep.selector)
    }

    pub fn bytecode_length(&self) -> usize {
        self.program.data_len()
    }

    pub fn bytecode_segment_lengths(&self) -> NestedIntList {
        NestedIntList::from(&self.bytecode_segment_felt_sizes)
    }

    pub fn get_entry_point(
        &self,
        entry_point: &EntryPointTypeAndSelector,
    ) -> Result<EntryPointV1, PreExecutionError> {
        self.entry_points_by_type.get_entry_point(entry_point)
    }

    /// Returns whether this contract should run using Cairo steps or Sierra gas.
    pub fn tracked_resource(&self, min_sierra_version: &SierraVersion) -> TrackedResource {
        if *min_sierra_version <= self.sierra_version {
            TrackedResource::SierraGas
        } else {
            TrackedResource::CairoSteps
        }
    }

    /// Returns the estimated VM resources required for computing Casm hash.
    /// This is an empiric measurement of several bytecode lengths, which constitutes as the
    /// dominant factor in it.
    fn estimate_casm_hash_computation_resources(&self) -> ExecutionResources {
        // TODO(AvivG): Pass `NestedMultipleInt` directly to avoid redundant `NestedIntList`
        // conversion.
        estimate_casm_poseidon_hash_computation_resources(&self.bytecode_segment_lengths())
    }

    /// Estimate the VM gas required to perform a CompiledClassHash migration.
    ///
    /// During the migration, both the blake hash, and the poseidon hash of the CASM are
    /// computed.
    ///
    /// Note: there's an assumption that the gas of the Blake hash is the same in both Stone and
    /// Stwo (i.e., the only builtin used is `range_check` and its gas is the same in both).
    ///
    /// Returns:
    /// - Total gas amount.
    /// - The builtins used in the Poseidon hash.
    fn estimate_compiled_class_hash_migration_resources(
        &self,
        versioned_constants: &VersionedConstants,
        blake_weight: usize,
    ) -> (GasAmount, BuiltinCounterMap) {
        let blake_hash_gas = estimate_casm_blake_hash_computation_resources(
            &self.bytecode_segment_felt_sizes,
            versioned_constants,
            blake_weight,
        );

        let poseidon_hash_resources =
            // TODO(AvivG): Pass `NestedMultipleInt` directly to avoid redundant `NestedIntList` conversion.
            estimate_casm_poseidon_hash_computation_resources(&self.bytecode_segment_lengths());
        let poseidon_hash_gas =
            vm_resources_to_sierra_gas(&poseidon_hash_resources, versioned_constants);

        (
            blake_hash_gas.checked_add_panic_on_overflow(poseidon_hash_gas),
            poseidon_hash_resources.builtin_instance_counter.clone(),
        )
    }

    // Returns the set of segments that were visited according to the given visited PCs.
    // Each visited segment must have its starting PC visited, and is represented by it.
    fn get_visited_segments(
        &self,
        visited_pcs: &HashSet<usize>,
    ) -> Result<Vec<usize>, TransactionExecutionError> {
        let mut reversed_visited_pcs: Vec<_> = visited_pcs.iter().cloned().sorted().rev().collect();
        // TODO(AvivG): Pass `NestedMultipleInt` directly to avoid redundant `NestedIntList`
        // conversion.
        get_visited_segments(&self.bytecode_segment_lengths(), &mut reversed_visited_pcs, &mut 0)
    }

    pub fn try_from_json_string(
        raw_contract_class: &str,
        sierra_version: SierraVersion,
    ) -> Result<CompiledClassV1, ProgramError> {
        let casm_contract_class: CasmContractClass = serde_json::from_str(raw_contract_class)?;
        let contract_class = CompiledClassV1::try_from((casm_contract_class, sierra_version))?;
        Ok(contract_class)
    }
}

impl HashableCompiledClass<EntryPointV1> for CompiledClassV1 {
    fn get_hashable_l1_entry_points(&self) -> &[EntryPointV1] {
        &self.entry_points_by_type.l1_handler
    }

    fn get_hashable_external_entry_points(&self) -> &[EntryPointV1] {
        &self.entry_points_by_type.external
    }

    fn get_hashable_constructor_entry_points(&self) -> &[EntryPointV1] {
        &self.entry_points_by_type.constructor
    }

    fn get_bytecode(&self) -> Vec<Felt> {
        self.program
            .iter_data()
            .map(|maybe_relocatable| match maybe_relocatable {
                MaybeRelocatable::Int(felt) => *felt,
                _ => panic!(
                    "Found MaybeRelocatable::RelocatableValue in the program data while trying to \
                     compute the compiled class hash. Expected all bytecode elements to be \
                     MaybeRelocatable::Int."
                ),
            })
            .collect()
    }

    // TODO(AvivG): Avoid unnecessary `NestedIntList` creation by having `HashableCompiledClass`
    // accept `NestedMultipleInt` via a shared trait.
    fn get_bytecode_segment_lengths(&self) -> Cow<'_, NestedIntList> {
        Cow::Owned(self.bytecode_segment_lengths())
    }
}

/// Returns the estimated VM resources required for computing Casm hash (for Cairo 1 contracts).
///
/// Note: the function focuses on the bytecode size, and currently ignores the cost handling the
/// class entry points.
/// Also, this function is not backward compatible.
pub fn estimate_casm_poseidon_hash_computation_resources(
    bytecode_segment_lengths: &NestedIntList,
) -> ExecutionResources {
    // The constants in this function were computed by running the Casm code on a few values
    // of `bytecode_segment_lengths`.
    match bytecode_segment_lengths {
        NestedIntList::Leaf(length) => {
            // The entire contract is a single segment (old Sierra contracts).
            &ExecutionResources {
                n_steps: 464,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 10)]),
            } + &poseidon_hash_many_cost(*length)
        }
        NestedIntList::Node(segments) => {
            // The contract code is segmented by its functions.
            let mut execution_resources = ExecutionResources {
                n_steps: 482,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 11)]),
            };
            let base_segment_cost = ExecutionResources {
                n_steps: 25,
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

/// Cost to hash a single flat segment of `len` felts.
fn leaf_cost(
    felt_size_groups: &FeltSizeGroups,
    versioned_constants: &VersionedConstants,
    blake_opcode_gas: usize,
) -> GasAmount {
    // All `len` inputs treated as “big” felts; no small-felt optimization here.
    cost_of_encode_felt252_data_and_calc_blake_hash(
        felt_size_groups.large,
        felt_size_groups.small,
        versioned_constants,
        blake_opcode_gas,
    )
}

/// Cost to hash a multi-segment contract:
fn node_cost(
    segs: &[NestedMultipleIntList],
    versioned_constants: &VersionedConstants,
    blake_opcode_gas: usize,
) -> GasAmount {
    // TODO(AvivG): Add base estimation for node.
    let mut gas = GasAmount::ZERO;

    // TODO(AvivG): Add base estimation of each segment. Could this be part of 'leaf_cost'?
    let segment_overhead = GasAmount::ZERO;

    // For each segment, hash its felts.
    for seg in segs {
        match seg {
            NestedMultipleIntList::Leaf(_, felt_size_groups) => {
                gas = gas.checked_add_panic_on_overflow(segment_overhead);
                gas = gas.checked_add_panic_on_overflow(leaf_cost(
                    felt_size_groups,
                    versioned_constants,
                    blake_opcode_gas,
                ));
            }
            _ => panic!("Estimating hash cost only supports at most one level of segmentation."),
        }
    }

    // Node‐level hash over (hash1, len1, hash2, len2, …): one segment hash (“big” felt))
    // and one segment length (“small” felt) per segment.
    let node_hash_cost = cost_of_encode_felt252_data_and_calc_blake_hash(
        segs.len(),
        segs.len(),
        versioned_constants,
        blake_opcode_gas,
    );

    gas.checked_add_panic_on_overflow(node_hash_cost)
}

/// Estimates the VM resources to compute the CASM Blake hash for a Cairo-1 contract:
/// - Uses only bytecode size.
pub fn estimate_casm_blake_hash_computation_resources(
    bytecode_segment_lengths: &NestedMultipleIntList,
    versioned_constants: &VersionedConstants,
    blake_opcode_gas: usize,
) -> GasAmount {
    // TODO(AvivG): Currently ignores entry-point hashing costs.
    // TODO(AvivG): Missing base overhead estimation for compiled_class_hash.

    // Basic frame overhead.
    // TODO(AvivG): Once compiled_class_hash estimation is complete,
    // revisit whether this should be moved into cost_of_encode_felt252_data_and_calc_blake_hash.
    let resources = ExecutionResources {
        n_steps: 0,
        n_memory_holes: 0,
        builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 3)]),
    };
    let gas = vm_resources_to_sierra_gas(&resources, versioned_constants);

    // Add leaf vs node cost
    let added_gas = match &bytecode_segment_lengths {
        // Single-segment contract (e.g., older Sierra contracts).
        NestedMultipleIntList::Leaf(_, felt_size_groups) => {
            leaf_cost(felt_size_groups, versioned_constants, blake_opcode_gas)
        }
        NestedMultipleIntList::Node(segs) => node_cost(segs, versioned_constants, blake_opcode_gas),
    };

    gas.checked_add_panic_on_overflow(added_gas)
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
    pub entry_points_by_type: EntryPointsByType<EntryPointV1>,
    pub hints: HashMap<String, Hint>,
    pub sierra_version: SierraVersion,
    bytecode_segment_felt_sizes: NestedMultipleIntList,
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

impl HasSelector for EntryPointV1 {
    fn selector(&self) -> &EntryPointSelector {
        &self.selector
    }
}

impl EntryPointHashable for EntryPointV1 {
    fn get_selector(&self) -> Felt {
        self.selector.0
    }
    fn get_offset(&self) -> Felt {
        Felt::from(self.offset.0)
    }
    fn get_builtins(&self) -> Vec<Felt> {
        self.builtins
            .iter()
            .map(|builtin| Felt::from_bytes_be_slice(builtin.to_str().as_bytes()))
            .collect_vec()
    }
}

impl TryFrom<VersionedCasm> for CompiledClassV1 {
    type Error = ProgramError;

    fn try_from((class, sierra_version): VersionedCasm) -> Result<Self, Self::Error> {
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

        let entry_points_by_type = EntryPointsByType {
            constructor: convert_entry_points_v1(&class.entry_points_by_type.constructor),
            external: convert_entry_points_v1(&class.entry_points_by_type.external),
            l1_handler: convert_entry_points_v1(&class.entry_points_by_type.l1_handler),
        };
        let bytecode_segment_lengths = class
            .bytecode_segment_lengths
            .unwrap_or_else(|| NestedIntList::Leaf(program.data_len()));

        let bytecode_segment_felt_sizes = NestedMultipleIntList::new(
            &bytecode_segment_lengths,
            &class.bytecode,
            program.data_len(),
        );

        Ok(CompiledClassV1(Arc::new(ContractClassV1Inner {
            program,
            entry_points_by_type,
            hints: string_to_hint,
            sierra_version,
            bytecode_segment_felt_sizes,
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
fn hint_to_hint_params(hint: &Hint) -> Result<HintParams, ProgramError> {
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

// TODO(Yoni): organize this file.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Modelled after [cairo_lang_starknet_classes::contract_class::ContractEntryPoints].
pub struct EntryPointsByType<EP: HasSelector> {
    pub constructor: Vec<EP>,
    pub external: Vec<EP>,
    pub l1_handler: Vec<EP>,
}

impl<EP: Clone + HasSelector> EntryPointsByType<EP> {
    pub fn get_entry_point(
        &self,
        entry_point: &EntryPointTypeAndSelector,
    ) -> Result<EP, PreExecutionError> {
        entry_point.verify_constructor()?;

        let entry_points_of_same_type = &self[entry_point.entry_point_type];
        let filtered_entry_points: Vec<_> = entry_points_of_same_type
            .iter()
            .filter(|ep| *ep.selector() == entry_point.entry_point_selector)
            .collect();

        match filtered_entry_points[..] {
            [] => Err(PreExecutionError::EntryPointNotFound(entry_point.entry_point_selector)),
            [entry_point] => Ok(entry_point.clone()),
            _ => Err(PreExecutionError::DuplicatedEntryPointSelector {
                selector: entry_point.entry_point_selector,
                typ: entry_point.entry_point_type,
            }),
        }
    }
}

impl<EP: HasSelector> Index<EntryPointType> for EntryPointsByType<EP> {
    type Output = Vec<EP>;

    fn index(&self, index: EntryPointType) -> &Self::Output {
        match index {
            EntryPointType::Constructor => &self.constructor,
            EntryPointType::External => &self.external,
            EntryPointType::L1Handler => &self.l1_handler,
        }
    }
}

impl NestedMultipleIntList {
    /// Builds a nested structure matching `layout`, consuming values from `bytecode`.
    #[allow(unused)]
    pub(crate) fn new(
        layout: &NestedIntList,
        bytecode: &[BigUintAsHex],
        bytecode_len: usize,
    ) -> Self {
        let (base_node, consumed_felts) = Self::new_inner(layout, bytecode);
        assert_eq!(consumed_felts, bytecode_len);
        base_node
    }

    /// Recursively builds the nested structure and returns it with the number of items consumed.
    fn new_inner(layout: &NestedIntList, bytecode: &[BigUintAsHex]) -> (Self, usize) {
        match layout {
            NestedIntList::Leaf(len) => {
                let felt_size_groups = FeltSizeGroups::from(&bytecode[..*len]);
                (NestedMultipleIntList::Leaf(*len, felt_size_groups), *len)
            }
            NestedIntList::Node(segments_vec) => {
                let mut total_felt_count = 0;
                let mut segments = Vec::with_capacity(segments_vec.len());

                for segment in segments_vec {
                    // Recurse into the segment layout.
                    let (segment, felt_count) =
                        Self::new_inner(segment, &bytecode[total_felt_count..]);
                    // Accumulate the count from the segment`s subtree.
                    total_felt_count += felt_count;
                    segments.push(segment);
                }

                (NestedMultipleIntList::Node(segments), total_felt_count)
            }
        }
    }
}
