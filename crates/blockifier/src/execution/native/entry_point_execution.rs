use cairo_native::execution_result::{BuiltinStats, ContractExecutionResult};
use cairo_native::utils::BuiltinCosts;
use cairo_vm::types::builtin_name::BuiltinName;

use crate::execution::call_info::{
    cairo_primitive_counter_map,
    CairoPrimitiveCounterMap,
    CallExecution,
    CallInfo,
    OpcodeName,
    Retdata,
};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{
    EntryPointExecutionContext,
    EntryPointExecutionResult,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::{EntryPointExecutionError, PostExecutionError, PreExecutionError};
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::execution::native::run_dispatch::run_native_executor;
use crate::execution::native::syscall_handler::NativeSyscallHandler;
use crate::state::state_api::State;
use crate::transaction::objects::ExecutionResourcesTraits;
use crate::utils::add_maps;

// todo(rodrigo): add an `entry point not found` test for Native
pub fn execute_entry_point_call(
    call: ExecutableCallEntryPoint,
    compiled_class: NativeCompiledClassV1,
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let entry_point = compiled_class.get_entry_point(&call.type_and_selector())?;

    let mut syscall_handler: NativeSyscallHandler<'_> =
        NativeSyscallHandler::new(call, state, context);

    let gas_costs = &syscall_handler.base.context.gas_costs();
    let builtin_costs = BuiltinCosts {
        // todo(rodrigo): Unsure of what value `const` means, but 1 is the right value.
        r#const: 1,
        pedersen: gas_costs.builtins.pedersen,
        bitwise: gas_costs.builtins.bitwise,
        ecop: gas_costs.builtins.ecop,
        poseidon: gas_costs.builtins.poseidon,
        add_mod: gas_costs.builtins.add_mod,
        mul_mod: gas_costs.builtins.mul_mod,
        blake: gas_costs.builtins.blake,
    };

    // Pre-charge entry point's initial budget to ensure sufficient gas for executing a minimal
    // entry point code. When redepositing is used, the entry point is aware of this pre-charge
    // and adjusts the gas counter accordingly if a smaller amount of gas is required.
    let initial_budget = syscall_handler.base.context.gas_costs().base.entry_point_initial_budget;
    let call_initial_gas = syscall_handler
        .base
        .call
        .initial_gas
        .checked_sub(initial_budget)
        .ok_or(PreExecutionError::InsufficientEntryPointGas)?;

    let execution_result = run_native_executor(
        &compiled_class.executor,
        entry_point.selector.0,
        &syscall_handler.base.call.calldata.0.clone(),
        call_initial_gas,
        builtin_costs,
        &mut syscall_handler,
    );

    syscall_handler.finalize();

    let call_result = execution_result.map_err(EntryPointExecutionError::NativeUnexpectedError)?;

    if let Some(error) = syscall_handler.unrecoverable_error {
        return Err(EntryPointExecutionError::NativeUnrecoverableError(Box::new(error)));
    }

    create_callinfo(call_result, syscall_handler)
}

fn create_callinfo(
    call_result: ContractExecutionResult,
    syscall_handler: NativeSyscallHandler<'_>,
) -> Result<CallInfo, EntryPointExecutionError> {
    let remaining_gas = call_result.remaining_gas;

    if remaining_gas > syscall_handler.base.call.initial_gas {
        return Err(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas. Used gas is greater than initial gas: {} > {}",
                remaining_gas, syscall_handler.base.call.initial_gas
            ),
        }
        .into());
    }

    let gas_consumed = syscall_handler.base.call.initial_gas - remaining_gas;
    let vm_resources = CallInfo::summarize_vm_resources(syscall_handler.base.inner_calls.iter());

    // Combine syscall builtins (no opcodes) with entry-point cairo primitives (builtins + opcodes).
    let version_constants = syscall_handler.base.context.versioned_constants();
    let syscall_builtins = version_constants
        .get_additional_os_syscall_resources(&syscall_handler.base.syscalls_usage)
        .filter_unused_builtins()
        .prover_builtins();
    let mut entry_point_primitive_counters =
        builtin_stats_to_primitive_counters(call_result.builtin_stats);
    add_maps(&mut entry_point_primitive_counters, &cairo_primitive_counter_map(syscall_builtins));

    Ok(CallInfo {
        call: syscall_handler.base.call.into(),
        execution: CallExecution {
            retdata: Retdata(call_result.return_values),
            events: syscall_handler.base.events,
            cairo_native: true,
            l2_to_l1_messages: syscall_handler.base.l2_to_l1_messages,
            failed: call_result.failure_flag,
            gas_consumed,
        },
        resources: vm_resources,
        inner_calls: syscall_handler.base.inner_calls,
        storage_access_tracker: syscall_handler.base.storage_access_tracker,
        tracked_resource: TrackedResource::SierraGas,
        #[cfg(feature = "benchmarking")]
        time: Default::default(),
        builtin_counters: entry_point_primitive_counters,
        syscalls_usage: syscall_handler.base.syscalls_usage,
    })
}

/// Converts native `BuiltinStats` into a unified `CairoPrimitiveCounterMap` (builtins + opcodes).
fn builtin_stats_to_primitive_counters(stats: BuiltinStats) -> CairoPrimitiveCounterMap {
    let builtins = [
        (BuiltinName::range_check, stats.range_check),
        (BuiltinName::pedersen, stats.pedersen),
        (BuiltinName::bitwise, stats.bitwise),
        (BuiltinName::ec_op, stats.ec_op),
        (BuiltinName::poseidon, stats.poseidon),
        (BuiltinName::range_check96, stats.range_check96),
        (BuiltinName::add_mod, stats.add_mod),
        (BuiltinName::mul_mod, stats.mul_mod),
    ];
    let opcodes = [(OpcodeName::blake, stats.blake)];

    builtins
        .into_iter()
        .map(|(builtin_name, count)| (builtin_name.into(), count))
        .chain(
            opcodes
                .into_iter()
                .map(|(opcode_name, count): (OpcodeName, _)| (opcode_name.into(), count)),
        )
        .filter(|(_, count)| *count > 0)
        .collect()
}
