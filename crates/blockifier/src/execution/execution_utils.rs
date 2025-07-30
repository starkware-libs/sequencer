use std::collections::HashMap;

use cairo_vm::serde::deserialize_program::{
    deserialize_array_of_bigint_hex,
    Attribute,
    HintParams,
    Identifier,
    ReferenceManager,
};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::types::program::Program;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner, ExecutionResources};
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::BigUint;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::Program as DeprecatedProgram;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::bouncer::vm_resources_to_sierra_gas;
use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::contract_class::{RunnableCompiledClass, TrackedResource};
use crate::execution::entry_point::{
    execute_constructor_entry_point,
    ConstructorContext,
    ConstructorEntryPointExecutionResult,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::{
    ConstructorEntryPointExecutionError,
    EntryPointExecutionError,
    PostExecutionError,
    PreExecutionError,
};
#[cfg(feature = "cairo_native")]
use crate::execution::native::entry_point_execution as native_entry_point_execution;
use crate::execution::stack_trace::{extract_trailing_cairo1_revert_trace, Cairo1RevertHeader};
use crate::execution::syscalls::hint_processor::{ENTRYPOINT_NOT_FOUND_ERROR, OUT_OF_GAS_ERROR};
use crate::execution::{deprecated_entry_point_execution, entry_point_execution};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::utils::u64_from_usize;

#[cfg(test)]
#[path = "execution_utils_test.rs"]
pub mod test;

pub type Args = Vec<CairoArg>;

pub const SEGMENT_ARENA_BUILTIN_SIZE: usize = 3;

/// A wrapper for execute_entry_point_call that performs pre and post-processing.
pub fn execute_entry_point_call_wrapper(
    mut call: ExecutableCallEntryPoint,
    compiled_class: RunnableCompiledClass,
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
    remaining_gas: &mut u64,
) -> EntryPointExecutionResult<CallInfo> {
    let current_tracked_resource = compiled_class.get_current_tracked_resource(context);
    if current_tracked_resource == TrackedResource::CairoSteps {
        // Override the initial gas with a high value so it won't limit the run.
        call.initial_gas = context.versioned_constants().infinite_gas_for_vm_mode();
    }
    let orig_call = call.clone();
    // Note: no return statements (explicit or implicit) should be added between the push and the
    // pop commands.
    context.tracked_resource_stack.push(current_tracked_resource);
    let res = execute_entry_point_call(call, compiled_class, state, context);
    context.tracked_resource_stack.pop().expect("Unexpected empty tracked resource.");

    match res {
        Ok(call_info) => {
            if call_info.execution.failed && !context.versioned_constants().enable_reverts {
                // Reverts are disabled.
                return Err(EntryPointExecutionError::ExecutionFailed {
                    error_trace: extract_trailing_cairo1_revert_trace(
                        &call_info,
                        Cairo1RevertHeader::Execution,
                    ),
                });
            }
            update_remaining_gas(remaining_gas, &call_info);
            Ok(call_info)
        }
        Err(EntryPointExecutionError::PreExecutionError(err))
            if context.versioned_constants().enable_reverts =>
        {
            let error_code = match err {
                PreExecutionError::EntryPointNotFound(_)
                | PreExecutionError::NoEntryPointOfTypeFound(_) => ENTRYPOINT_NOT_FOUND_ERROR,
                PreExecutionError::InsufficientEntryPointGas => OUT_OF_GAS_ERROR,
                _ => return Err(err.into()),
            };
            Ok(CallInfo {
                call: orig_call.into(),
                execution: CallExecution {
                    retdata: Retdata(vec![Felt::from_hex(error_code).unwrap()]),
                    // FIXME: Should we get the `is_cairo_native` bool?
                    failed: true,
                    gas_consumed: 0,
                    ..CallExecution::default()
                },
                tracked_resource: current_tracked_resource,
                ..CallInfo::default()
            })
        }
        Err(err) => Err(err),
    }
}

/// Executes a specific call to a contract entry point and returns its output.
pub fn execute_entry_point_call(
    call: ExecutableCallEntryPoint,
    compiled_class: RunnableCompiledClass,
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    match compiled_class {
        RunnableCompiledClass::V0(compiled_class) => {
            deprecated_entry_point_execution::execute_entry_point_call(
                call,
                compiled_class,
                state,
                context,
            )
        }
        RunnableCompiledClass::V1(compiled_class) => {
            entry_point_execution::execute_entry_point_call(call, compiled_class, state, context)
        }
        #[cfg(feature = "cairo_native")]
        RunnableCompiledClass::V1Native(compiled_class) => {
            if context.tracked_resource_stack.last() == Some(&TrackedResource::CairoSteps) {
                // We cannot run native with cairo steps as the tracked resources (it's a vm
                // resouorce).
                entry_point_execution::execute_entry_point_call(
                    call,
                    compiled_class.casm(),
                    state,
                    context,
                )
            } else {
                native_entry_point_execution::execute_entry_point_call(
                    call,
                    compiled_class,
                    state,
                    context,
                )
            }
        }
    }
}

pub fn update_remaining_gas(remaining_gas: &mut u64, call_info: &CallInfo) {
    *remaining_gas -= call_info.execution.gas_consumed;
}

pub fn read_execution_retdata(
    runner: &CairoRunner,
    retdata_size: MaybeRelocatable,
    retdata_ptr: &MaybeRelocatable,
) -> Result<Retdata, PostExecutionError> {
    let retdata_size = match retdata_size {
        MaybeRelocatable::Int(retdata_size) => usize::try_from(retdata_size.to_bigint())
            .map_err(PostExecutionError::RetdataSizeTooBig)?,
        relocatable => {
            return Err(VirtualMachineError::ExpectedIntAtRange(Box::new(Some(relocatable))).into());
        }
    };

    Ok(Retdata(felt_range_from_ptr(&runner.vm, Relocatable::try_from(retdata_ptr)?, retdata_size)?))
}

pub fn relocatable_from_ptr(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> Result<Relocatable, VirtualMachineError> {
    let value = vm.get_relocatable(*ptr)?;
    *ptr = (*ptr + 1)?;
    Ok(value)
}

pub fn felt_from_ptr(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> Result<Felt, VirtualMachineError> {
    let felt = vm.get_integer(*ptr)?.into_owned();
    *ptr = (*ptr + 1)?;
    Ok(felt)
}

pub fn write_u256(
    vm: &mut VirtualMachine,
    ptr: &mut Relocatable,
    value: BigUint,
) -> Result<(), MemoryError> {
    write_felt(vm, ptr, Felt::from(&value & BigUint::from(u128::MAX)))?;
    write_felt(vm, ptr, Felt::from(value >> 128))
}

pub fn felt_range_from_ptr(
    vm: &VirtualMachine,
    ptr: Relocatable,
    size: usize,
) -> Result<Vec<Felt>, VirtualMachineError> {
    let values = vm.get_integer_range(ptr, size)?;
    // Extract values as `Felt`.
    let values = values.into_iter().map(|felt| *felt).collect();
    Ok(values)
}

// TODO(Elin,01/05/2023): aim to use LC's implementation once it's in a separate crate.
pub fn sn_api_to_cairo_vm_program(program: DeprecatedProgram) -> Result<Program, ProgramError> {
    let identifiers = serde_json::from_value::<HashMap<String, Identifier>>(program.identifiers)?;
    let builtins = serde_json::from_value(program.builtins)?;
    let data = deserialize_array_of_bigint_hex(program.data)?;
    let hints = serde_json::from_value::<HashMap<usize, Vec<HintParams>>>(program.hints)?;
    let main = None;
    let error_message_attributes = match program.attributes {
        serde_json::Value::Null => vec![],
        attributes => serde_json::from_value::<Vec<Attribute>>(attributes)?
            .into_iter()
            .filter(|attr| attr.name == "error_message")
            .collect(),
    };

    let instruction_locations = None;
    let reference_manager = serde_json::from_value::<ReferenceManager>(program.reference_manager)?;

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

    Ok(program)
}

#[derive(Debug)]
// Invariant: read-only.
pub struct ReadOnlySegment {
    pub start_ptr: Relocatable,
    pub length: usize,
}

/// Represents read-only segments dynamically allocated during execution.
#[derive(Debug, Default)]
// Invariant: read-only.
pub struct ReadOnlySegments(Vec<ReadOnlySegment>);

impl ReadOnlySegments {
    pub fn allocate(
        &mut self,
        vm: &mut VirtualMachine,
        data: &[MaybeRelocatable],
    ) -> Result<Relocatable, MemoryError> {
        let start_ptr = vm.add_memory_segment();
        self.0.push(ReadOnlySegment { start_ptr, length: data.len() });
        vm.load_data(start_ptr, data)?;
        Ok(start_ptr)
    }

    pub fn validate(&self, vm: &VirtualMachine) -> Result<(), PostExecutionError> {
        for segment in &self.0 {
            let used_size = vm
                .get_segment_used_size(
                    segment
                        .start_ptr
                        .segment_index
                        .try_into()
                        .expect("The size of isize and usize should be the same."),
                )
                .expect("Segments must contain the allocated read-only segment.");
            if segment.length != used_size {
                return Err(PostExecutionError::SecurityValidationError(
                    "Read-only segments".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn mark_as_accessed(&self, runner: &mut CairoRunner) -> Result<(), PostExecutionError> {
        for segment in &self.0 {
            runner.vm.mark_address_range_as_accessed(segment.start_ptr, segment.length)?;
        }

        Ok(())
    }
}

/// Instantiates the given class and assigns it an address.
/// Returns the call info of the deployed class' constructor execution.
pub fn execute_deployment(
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
    ctor_context: ConstructorContext,
    constructor_calldata: Calldata,
    remaining_gas: &mut u64,
) -> ConstructorEntryPointExecutionResult<CallInfo> {
    // Address allocation in the state is done before calling the constructor, so that it is
    // visible from it.
    let deployed_contract_address = ctor_context.storage_address;
    let current_class_hash =
        state.get_class_hash_at(deployed_contract_address).map_err(|error| {
            ConstructorEntryPointExecutionError::new(error.into(), &ctor_context, None)
        })?;
    if current_class_hash != ClassHash::default() {
        return Err(ConstructorEntryPointExecutionError::new(
            StateError::UnavailableContractAddress(deployed_contract_address).into(),
            &ctor_context,
            None,
        ));
    }

    state.set_class_hash_at(deployed_contract_address, ctor_context.class_hash).map_err(
        |error| ConstructorEntryPointExecutionError::new(error.into(), &ctor_context, None),
    )?;

    execute_constructor_entry_point(
        state,
        context,
        ctor_context,
        constructor_calldata,
        remaining_gas,
    )
}

pub fn write_felt(
    vm: &mut VirtualMachine,
    ptr: &mut Relocatable,
    felt: Felt,
) -> Result<(), MemoryError> {
    write_maybe_relocatable(vm, ptr, felt)
}

pub fn write_maybe_relocatable<T: Into<MaybeRelocatable>>(
    vm: &mut VirtualMachine,
    ptr: &mut Relocatable,
    value: T,
) -> Result<(), MemoryError> {
    vm.insert_value(*ptr, value)?;
    *ptr = (*ptr + 1)?;
    Ok(())
}

/// Returns the VM resources required for running `poseidon_hash_many` in the Starknet OS.
pub fn poseidon_hash_many_cost(data_length: usize) -> ExecutionResources {
    ExecutionResources {
        n_steps: (data_length / 10) * 55
            + ((data_length % 10) / 2) * 18
            + (data_length % 2) * 3
            + 21,
        n_memory_holes: 0,
        builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, data_length / 2 + 1)]),
    }
}

// Constants that define how felts are encoded into u32s for BLAKE hashing.
mod blake_encoding {
    /// Number of u32s in a Blake input message.
    pub const N_U32S_MESSAGE: usize = 16;

    /// Number of u32s a felt is encoded into.
    pub const N_U32S_BIG_FELT: usize = 8;
    pub const N_U32S_SMALL_FELT: usize = 2;
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

    // BLAKE opcode gas cost in Stwo.
    // TODO(AvivG): Remove this and use bouncer's blake_weight.
    pub const BLAKE_OPCODE_GAS: usize = 0;
}

/// Calculates the total number of u32s required to encode the given number of big and small felts.
/// Big felts encode to 8 u32s each, small felts encode to 2 u32s each.
fn total_u32s_from_felts(n_big_felts: usize, n_small_felts: usize) -> usize {
    let big_u32s = n_big_felts
        .checked_mul(blake_encoding::N_U32S_BIG_FELT)
        .expect("Overflow computing big felts u32s");
    let small_u32s = n_small_felts
        .checked_mul(blake_encoding::N_U32S_SMALL_FELT)
        .expect("Overflow computing small felts u32s");
    big_u32s.checked_add(small_u32s).expect("Overflow computing total u32s")
}

fn base_steps_for_blake_hash(n_u32s: usize) -> usize {
    let rem_u32s = n_u32s % blake_encoding::N_U32S_MESSAGE;
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
fn compute_blake_hash_steps(n_big_felts: usize, n_small_felts: usize) -> usize {
    let total_u32s = total_u32s_from_felts(n_big_felts, n_small_felts);
    if total_u32s == 0 {
        // The empty input case is a special case.
        return blake_estimation::STEPS_EMPTY_INPUT;
    }

    let base_steps = base_steps_for_blake_hash(total_u32s);
    let felt_steps = felts_steps(n_big_felts, n_small_felts);

    base_steps.checked_add(felt_steps).expect("Overflow computing total Blake hash steps")
}

/// Returns the number of BLAKE opcodes needed to hash the given felts.
/// Each BLAKE opcode processes 16 u32s (partial messages are padded).
fn count_blake_opcode(n_big_felts: usize, n_small_felts: usize) -> usize {
    // Count the total number of u32s to be hashed.
    let total_u32s = total_u32s_from_felts(n_big_felts, n_small_felts);
    total_u32s.div_ceil(blake_encoding::N_U32S_MESSAGE)
}

pub fn encode_and_blake_hash_execution_resources(
    n_big_felts: usize,
    n_small_felts: usize,
) -> ExecutionResources {
    let n_steps = compute_blake_hash_steps(n_big_felts, n_small_felts);
    let n_felts =
        n_big_felts.checked_add(n_small_felts).expect("Overflow computing total number of felts");

    let builtin_instance_counter = match n_felts {
        // The empty case does not use builtins at all.
        0 => HashMap::new(),
        // One `range_check` per input felt to validate its size + Overhead for the non empty case.
        _ => HashMap::from([(
            BuiltinName::range_check,
            n_felts + blake_estimation::BASE_RANGE_CHECK_NON_EMPTY,
        )]),
    };

    ExecutionResources { n_steps, n_memory_holes: 0, builtin_instance_counter }
}

/// Estimates the L2 gas for `encode_felt252_data_and_calc_blake_hash` in the Starknet OS.
///
/// This includes:
/// - Gas derived from `ExecutionResources`
/// - Additional gas for Blake opcodes (which is **not** part of `ExecutionResources`)
///
/// # Encoding Details
/// - Small felts → 2 `u32`s each; Big felts → 8 `u32`s each.
/// - Each felt requires one `range_check` operation.
///
/// # Compatibility Assumptions
/// This function is used for both Stwo ("proving_gas") and Stone ("sierra_gas") estimations.
/// This unified logic is valid because the only builtin involved is `range_check`, which has
/// the same cost in both provers (see usage in `bouncer.get_tx_weights`).
///
/// # Notes
/// The Blake opcode gas is separated because it is not reported via `ExecutionResources`,
/// and must be accounted for explicitly.
// TODO(AvivG): Consider separating `encode_felt252_to_u32s` and `blake_with_opcode` costs for
// improved granularity and testability.
pub fn cost_of_encode_felt252_data_and_calc_blake_hash(
    n_big_felts: usize,
    n_small_felts: usize,
    versioned_constants: &VersionedConstants,
) -> GasAmount {
    let vm_resources = encode_and_blake_hash_execution_resources(n_big_felts, n_small_felts);

    assert!(
        vm_resources.builtin_instance_counter.keys().all(|&k| k == BuiltinName::range_check),
        "Expected either empty builtins or only `range_check` builtin, got: {:?}. This breaks the \
         assumption that builtin costs are identical between provers.",
        vm_resources.builtin_instance_counter.keys().collect::<Vec<_>>()
    );

    let vm_gas = vm_resources_to_sierra_gas(&vm_resources, versioned_constants);

    let blake_opcode_count = count_blake_opcode(n_big_felts, n_small_felts);
    let blake_opcode_gas = blake_opcode_count
        .checked_mul(blake_estimation::BLAKE_OPCODE_GAS)
        .map(u64_from_usize)
        .map(GasAmount)
        .expect("Overflow computing Blake opcode gas.");

    vm_gas.checked_add_panic_on_overflow(blake_opcode_gas)
}
