use std::collections::HashSet;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::cairo_runner::{CairoArg, CairoRunner};
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::abi::constants::{CONSTRUCTOR_ENTRY_POINT_NAME, DEFAULT_ENTRY_POINT_SELECTOR};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::EntryPointSelector;
use starknet_api::hash::StarkHash;

use super::call_info::StorageAccessTracker;
use super::execution_utils::SEGMENT_ARENA_BUILTIN_SIZE;
use crate::execution::call_info::{CallExecution, CallInfo};
use crate::execution::contract_class::{CompiledClassV0, TrackedResource};
use crate::execution::deprecated_syscalls::deprecated_syscall_executor::DeprecatedSyscallExecutor;
use crate::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallHintProcessor;
use crate::execution::entry_point::{
    EntryPointExecutionContext,
    EntryPointExecutionResult,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::{PostExecutionError, PreExecutionError};
use crate::execution::execution_utils::{read_execution_retdata, Args, ReadOnlySegments};
use crate::state::state_api::State;
use crate::transaction::objects::ExecutionResourcesTraits;

pub struct VmExecutionContext<'a> {
    pub runner: CairoRunner,
    pub syscall_handler: DeprecatedSyscallHintProcessor<'a>,
    pub initial_syscall_ptr: Relocatable,
    pub entry_point_pc: usize,
}

pub const CAIRO0_BUILTINS_NAMES: [BuiltinName; 6] = [
    BuiltinName::range_check,
    BuiltinName::pedersen,
    BuiltinName::ecdsa,
    BuiltinName::bitwise,
    BuiltinName::ec_op,
    BuiltinName::poseidon,
];

/// Executes a specific call to a contract entry point and returns its output.
pub fn execute_entry_point_call(
    call: ExecutableCallEntryPoint,
    compiled_class: CompiledClassV0,
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let VmExecutionContext { mut runner, mut syscall_handler, initial_syscall_ptr, entry_point_pc } =
        initialize_execution_context(&call, compiled_class, state, context)?;

    let (implicit_args, args) = prepare_call_arguments(
        &call,
        &mut runner,
        initial_syscall_ptr,
        &mut syscall_handler.read_only_segments,
    )?;
    let n_total_args = args.len();

    // Execute.
    run_entry_point(&mut runner, &mut syscall_handler, entry_point_pc, args)?;

    Ok(finalize_execution(runner, syscall_handler, call, implicit_args, n_total_args)?)
}

pub fn initialize_execution_context<'a>(
    call: &ExecutableCallEntryPoint,
    compiled_class: CompiledClassV0,
    state: &'a mut dyn State,
    context: &'a mut EntryPointExecutionContext,
) -> Result<VmExecutionContext<'a>, PreExecutionError> {
    // Verify use of cairo0 builtins only.
    let program_builtins: HashSet<&BuiltinName> =
        HashSet::from_iter(compiled_class.program.iter_builtins());
    let unsupported_builtins =
        &program_builtins - &HashSet::from_iter(CAIRO0_BUILTINS_NAMES.iter());
    if !unsupported_builtins.is_empty() {
        return Err(PreExecutionError::UnsupportedCairo0Builtin(
            unsupported_builtins.iter().map(|&item| *item).collect(),
        ));
    }

    // Resolve initial PC from EP indicator.
    let entry_point_pc = resolve_entry_point_pc(call, &compiled_class)?;
    // Instantiate Cairo runner.
    let proof_mode = false;
    let trace_enabled = false;
    let dynamic_layout_params = None;
    let allow_missing_builtins = false;
    let disable_trace_padding = false;
    let program_base = None;
    let mut runner = CairoRunner::new(
        &compiled_class.program,
        LayoutName::starknet,
        dynamic_layout_params,
        proof_mode,
        trace_enabled,
        disable_trace_padding,
    )?;

    runner.initialize_builtins(allow_missing_builtins)?;
    runner.initialize_segments(program_base);

    // Instantiate syscall handler.
    let initial_syscall_ptr = runner.vm.add_memory_segment();
    let syscall_handler = DeprecatedSyscallHintProcessor::new(
        state,
        context,
        initial_syscall_ptr,
        call.storage_address,
        call.caller_address,
        call.class_hash,
    );

    Ok(VmExecutionContext { runner, syscall_handler, initial_syscall_ptr, entry_point_pc })
}

pub fn resolve_entry_point_pc(
    call: &ExecutableCallEntryPoint,
    compiled_class: &CompiledClassV0,
) -> Result<usize, PreExecutionError> {
    if call.entry_point_type == EntryPointType::Constructor
        && call.entry_point_selector != selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME)
    {
        return Err(PreExecutionError::InvalidConstructorEntryPointName);
    }

    let entry_points_of_same_type = &compiled_class.entry_points_by_type[&call.entry_point_type];
    let filtered_entry_points: Vec<_> = entry_points_of_same_type
        .iter()
        .filter(|ep| ep.selector == call.entry_point_selector)
        .collect();

    // Returns the default entrypoint if the given selector is missing.
    if filtered_entry_points.is_empty() {
        match entry_points_of_same_type.first() {
            Some(entry_point) => {
                if entry_point.selector
                    == EntryPointSelector(StarkHash::from(DEFAULT_ENTRY_POINT_SELECTOR))
                {
                    return Ok(entry_point.offset.0);
                } else {
                    return Err(PreExecutionError::EntryPointNotFound(call.entry_point_selector));
                }
            }
            None => {
                return Err(PreExecutionError::NoEntryPointOfTypeFound(call.entry_point_type));
            }
        }
    }

    if filtered_entry_points.len() > 1 {
        return Err(PreExecutionError::DuplicatedEntryPointSelector {
            selector: call.entry_point_selector,
            typ: call.entry_point_type,
        });
    }

    // Filtered entry points contain exactly one element.
    let entry_point = filtered_entry_points
        .first()
        .expect("The number of entry points with the given selector is exactly one.");
    Ok(entry_point.offset.0)
}

pub fn prepare_call_arguments(
    call: &ExecutableCallEntryPoint,
    runner: &mut CairoRunner,
    initial_syscall_ptr: Relocatable,
    read_only_segments: &mut ReadOnlySegments,
) -> Result<(Vec<MaybeRelocatable>, Args), PreExecutionError> {
    let mut args: Args = vec![];

    // Prepare called EP details.
    let entry_point_selector = MaybeRelocatable::from(call.entry_point_selector.0);
    args.push(CairoArg::from(entry_point_selector));

    // Prepare implicit arguments.
    let mut implicit_args = vec![];
    implicit_args.push(MaybeRelocatable::from(initial_syscall_ptr));
    implicit_args.extend(
        runner
            .vm
            .get_builtin_runners()
            .iter()
            .flat_map(|builtin_runner| builtin_runner.initial_stack()),
    );
    args.push(CairoArg::from(implicit_args.clone()));

    // Prepare calldata arguments.
    let calldata = &call.calldata.0;
    let calldata: Vec<MaybeRelocatable> =
        calldata.iter().map(|&arg| MaybeRelocatable::from(arg)).collect();
    let calldata_length = MaybeRelocatable::from(calldata.len());
    args.push(CairoArg::from(calldata_length));

    let calldata_start_ptr =
        MaybeRelocatable::from(read_only_segments.allocate(&mut runner.vm, &calldata)?);
    args.push(CairoArg::from(calldata_start_ptr));

    Ok((implicit_args, args))
}

/// Runs the runner from the given PC.
pub fn run_entry_point(
    runner: &mut CairoRunner,
    hint_processor: &mut DeprecatedSyscallHintProcessor<'_>,
    entry_point_pc: usize,
    args: Args,
) -> EntryPointExecutionResult<()> {
    let verify_secure = true;
    let program_segment_size = None; // Infer size from program.
    let args: Vec<&CairoArg> = args.iter().collect();
    let result = runner.run_from_entrypoint(
        entry_point_pc,
        &args,
        verify_secure,
        program_segment_size,
        hint_processor,
    );

    Ok(result?)
}

pub fn finalize_execution(
    mut runner: CairoRunner,
    syscall_handler: DeprecatedSyscallHintProcessor<'_>,
    call: ExecutableCallEntryPoint,
    implicit_args: Vec<MaybeRelocatable>,
    n_total_args: usize,
) -> Result<CallInfo, PostExecutionError> {
    // Close memory holes in segments (OS code touches those memory cells, we simulate it).
    let initial_fp = runner
        .get_initial_fp()
        .expect("The initial_fp field should be initialized after running the entry point.");
    // When execution starts the stack holds the EP arguments + [ret_fp, ret_pc].
    let args_ptr = (initial_fp - (n_total_args + 2))?;
    runner.vm.mark_address_range_as_accessed(args_ptr, n_total_args)?;
    syscall_handler.read_only_segments.mark_as_accessed(&mut runner)?;

    // Validate run.
    let [retdata_size, retdata_ptr]: [MaybeRelocatable; 2] =
        runner.vm.get_return_values(2)?.try_into().expect("Return values must be of size 2.");
    let implicit_args_end_ptr = (runner.vm.get_ap() - 2)?;
    validate_run(&mut runner, &syscall_handler, implicit_args, implicit_args_end_ptr)?;

    // Take into account the VM execution resources of the current call, without inner calls.
    // Has to happen after marking holes in segments as accessed.
    let mut vm_resources_without_inner_calls = runner
        .get_execution_resources()
        .map_err(VirtualMachineError::RunnerError)?
        .filter_unused_builtins();
    let versioned_constants = syscall_handler.context.versioned_constants();
    if versioned_constants.segment_arena_cells {
        vm_resources_without_inner_calls
            .builtin_instance_counter
            .get_mut(&BuiltinName::segment_arena)
            .map_or_else(|| {}, |val| *val *= SEGMENT_ARENA_BUILTIN_SIZE);
    }
    // Take into account the syscall resources of the current call.
    vm_resources_without_inner_calls +=
        &versioned_constants.get_additional_os_syscall_resources(&syscall_handler.syscalls_usage);

    let vm_resources = &vm_resources_without_inner_calls
        + &CallInfo::summarize_vm_resources(syscall_handler.inner_calls.iter());

    Ok(CallInfo {
        call: call.into(),
        execution: CallExecution {
            retdata: read_execution_retdata(&runner, retdata_size, &retdata_ptr)?,
            events: syscall_handler.events,
            l2_to_l1_messages: syscall_handler.l2_to_l1_messages,
            cairo_native: false,
            failed: false,
            gas_consumed: 0,
        },
        inner_calls: syscall_handler.inner_calls,
        tracked_resource: TrackedResource::CairoSteps,
        resources: vm_resources,
        storage_access_tracker: StorageAccessTracker {
            storage_read_values: syscall_handler.read_values,
            accessed_storage_keys: syscall_handler.accessed_keys,
            ..Default::default()
        },
        builtin_counters: vm_resources_without_inner_calls.prover_builtins(),
    })
}

pub fn validate_run(
    runner: &mut CairoRunner,
    syscall_handler: &DeprecatedSyscallHintProcessor<'_>,
    implicit_args: Vec<MaybeRelocatable>,
    implicit_args_end: Relocatable,
) -> Result<(), PostExecutionError> {
    // Validate builtins' final stack.
    let mut current_builtin_ptr = implicit_args_end;
    current_builtin_ptr = runner.get_builtins_final_stack(current_builtin_ptr)?;

    // Validate implicit arguments segment length is unchanged.
    // Subtract one to get to the first implicit arg segment (the syscall pointer).
    let implicit_args_start = (current_builtin_ptr - 1)?;
    if (implicit_args_start + implicit_args.len())? != implicit_args_end {
        return Err(PostExecutionError::SecurityValidationError(
            "Implicit arguments' segments".to_string(),
        ));
    }

    // Validate syscall segment start.
    let syscall_start_ptr = implicit_args.first().expect("Implicit args must not be empty.");
    let syscall_start_ptr = Relocatable::try_from(syscall_start_ptr)?;
    if syscall_start_ptr.offset != 0 {
        return Err(PostExecutionError::SecurityValidationError(
            "Syscall segment start".to_string(),
        ));
    }

    // Validate syscall segment size.
    let syscall_end_ptr = runner.vm.get_relocatable(implicit_args_start)?;
    let syscall_used_size = runner
        .vm
        .get_segment_used_size(
            syscall_start_ptr
                .segment_index
                .try_into()
                .expect("The size of isize and usize should be the same."),
        )
        .expect("Segments must contain the syscall segment.");
    if (syscall_start_ptr + syscall_used_size)? != syscall_end_ptr {
        return Err(PostExecutionError::SecurityValidationError(
            "Syscall segment size".to_string(),
        ));
    }

    // Validate syscall segment end.
    syscall_handler.verify_syscall_ptr(syscall_end_ptr).map_err(|_| {
        PostExecutionError::SecurityValidationError("Syscall segment end".to_string())
    })?;

    syscall_handler.read_only_segments.validate(&runner.vm)
}
