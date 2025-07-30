use cairo_native::execution_result::{BuiltinStats, ContractExecutionResult};
use cairo_native::utils::BuiltinCosts;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use crate::execution::call_info::{BuiltinCounterMap, CallExecution, CallInfo, Retdata};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{
    EntryPointExecutionContext,
    EntryPointExecutionResult,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::{EntryPointExecutionError, PostExecutionError, PreExecutionError};
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::execution::native::syscall_handler::NativeSyscallHandler;
use crate::state::state_api::State;

// todo(rodrigo): add an `entry point not found` test for Native
#[allow(clippy::result_large_err)]
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

    let execution_result = compiled_class.executor.run(
        entry_point.selector.0,
        &syscall_handler.base.call.calldata.0.clone(),
        call_initial_gas,
        Some(builtin_costs),
        &mut syscall_handler,
    );

    syscall_handler.finalize();

    let call_result = execution_result.map_err(EntryPointExecutionError::NativeUnexpectedError)?;

    if let Some(error) = syscall_handler.unrecoverable_error {
        return Err(EntryPointExecutionError::NativeUnrecoverableError(Box::new(error)));
    }

    create_callinfo(call_result, syscall_handler)
}

#[allow(clippy::result_large_err)]
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

    // Retrieve the builtin counts from the syscall handler
    let version_constants = syscall_handler.base.context.versioned_constants();
    let syscall_resources =
        version_constants.get_additional_os_syscall_resources(&syscall_handler.base.syscalls_usage);

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
        builtin_counters: builtin_stats_to_builtin_counter_map(
            call_result.builtin_stats,
            syscall_resources,
        ),
    })
}

fn builtin_stats_to_builtin_counter_map(
    builtin_stats: BuiltinStats,
    syscall_resources: ExecutionResources,
) -> BuiltinCounterMap {
    let syscall_builtin_counts = &syscall_resources.builtin_instance_counter;

    // Helper function to add builtin counts
    let add_builtin = |builtin: BuiltinName, stats_count: usize| -> Option<(BuiltinName, usize)> {
        let total = stats_count + syscall_builtin_counts.get(&builtin).copied().unwrap_or_default();
        (total > 0).then_some((builtin, total))
    };

    [
        // Builtins with corresponding fields in BuiltinStats
        add_builtin(BuiltinName::range_check, builtin_stats.range_check),
        add_builtin(BuiltinName::pedersen, builtin_stats.pedersen),
        add_builtin(BuiltinName::bitwise, builtin_stats.bitwise),
        add_builtin(BuiltinName::ec_op, builtin_stats.ec_op),
        add_builtin(BuiltinName::poseidon, builtin_stats.poseidon),
        add_builtin(BuiltinName::range_check96, builtin_stats.range_check96),
        add_builtin(BuiltinName::add_mod, builtin_stats.add_mod),
        add_builtin(BuiltinName::mul_mod, builtin_stats.mul_mod),
        // Syscall-only builtins
        add_builtin(BuiltinName::ecdsa, 0),
        add_builtin(BuiltinName::keccak, 0),
    ]
    .into_iter()
    .flatten()
    .collect()
}
