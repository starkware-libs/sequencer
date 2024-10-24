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
use starknet_api::transaction::Calldata;
use starknet_types_core::felt::Felt;

use super::call_info::CallExecution;
use super::entry_point::ConstructorEntryPointExecutionResult;
use super::errors::{
    ConstructorEntryPointExecutionError,
    EntryPointExecutionError,
    PreExecutionError,
};
use super::syscalls::hint_processor::ENTRYPOINT_NOT_FOUND_ERROR;
use crate::execution::call_info::{CallInfo, Retdata};
use crate::execution::contract_class::{ContractClass, TrackedResource};
use crate::execution::entry_point::{
    execute_constructor_entry_point,
    CallEntryPoint,
    ConstructorContext,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
};
use crate::execution::errors::PostExecutionError;
#[cfg(feature = "cairo_native")]
use crate::execution::native::entry_point_execution as native_entry_point_execution;
use crate::execution::stack_trace::extract_trailing_cairo1_revert_trace;
use crate::execution::{deprecated_entry_point_execution, entry_point_execution};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::transaction::objects::TransactionInfo;

pub type Args = Vec<CairoArg>;

pub const SEGMENT_ARENA_BUILTIN_SIZE: usize = 3;

/// A wrapper for execute_entry_point_call that performs pre and post-processing.
pub fn execute_entry_point_call_wrapper(
    mut call: CallEntryPoint,
    contract_class: ContractClass,
    state: &mut dyn State,
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
    remaining_gas: &mut u64,
) -> EntryPointExecutionResult<CallInfo> {
    let contract_tracked_resource = contract_class
        .tracked_resource(&context.versioned_constants().min_compiler_version_for_sierra_gas);
    // Note: no return statements (explicit or implicit) should be added between the push and the
    // pop commands.

    // Once we ran with CairoSteps, we will continue to run using it for all nested calls.
    match context.tracked_resource_stack.last() {
        Some(TrackedResource::CairoSteps) => {
            context.tracked_resource_stack.push(TrackedResource::CairoSteps)
        }
        Some(TrackedResource::SierraGas) => {
            if contract_tracked_resource == TrackedResource::CairoSteps {
                // Switching from SierraGas to CairoSteps: override initial_gas with a high value so
                // it won't limit the run.
                call.initial_gas = context.versioned_constants().default_initial_gas_cost();
            };
            context.tracked_resource_stack.push(contract_tracked_resource)
        }
        None => context.tracked_resource_stack.push(contract_tracked_resource),
    };

    let orig_call = call.clone();
    let res = execute_entry_point_call(call, contract_class, state, resources, context);
    let current_tracked_resource =
        context.tracked_resource_stack.pop().expect("Unexpected empty tracked resource.");

    match res {
        Ok(call_info) => {
            if call_info.execution.failed && !context.versioned_constants().enable_reverts {
                // Reverts are disabled.
                return Err(EntryPointExecutionError::ExecutionFailed {
                    error_trace: extract_trailing_cairo1_revert_trace(&call_info),
                });
            }
            update_remaining_gas(remaining_gas, &call_info);
            Ok(call_info)
        }
        Err(EntryPointExecutionError::PreExecutionError(
            PreExecutionError::EntryPointNotFound(_)
            | PreExecutionError::NoEntryPointOfTypeFound(_),
        )) if context.versioned_constants().enable_reverts => Ok(CallInfo {
            call: orig_call,
            execution: CallExecution {
                retdata: Retdata(vec![Felt::from_hex(ENTRYPOINT_NOT_FOUND_ERROR).unwrap()]),
                failed: true,
                gas_consumed: 0,
                ..CallExecution::default()
            },
            tracked_resource: current_tracked_resource,
            ..CallInfo::default()
        }),
        Err(err) => Err(err),
    }
}

/// Executes a specific call to a contract entry point and returns its output.
pub fn execute_entry_point_call(
    call: CallEntryPoint,
    contract_class: ContractClass,
    state: &mut dyn State,
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    match contract_class {
        ContractClass::V0(contract_class) => {
            deprecated_entry_point_execution::execute_entry_point_call(
                call,
                contract_class,
                state,
                resources,
                context,
            )
        }
        ContractClass::V1(contract_class) => entry_point_execution::execute_entry_point_call(
            call,
            contract_class,
            state,
            resources,
            context,
        ),
        #[cfg(feature = "cairo_native")]
        ContractClass::V1Native(contract_class) => {
            native_entry_point_execution::execute_entry_point_call(
                call,
                contract_class,
                state,
                resources,
                context,
            )
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
    resources: &mut ExecutionResources,
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
        resources,
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

pub fn max_fee_for_execution_info(tx_info: &TransactionInfo) -> Felt {
    match tx_info {
        TransactionInfo::Current(_) => 0,
        TransactionInfo::Deprecated(tx_info) => tx_info.max_fee.0,
    }
    .into()
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
