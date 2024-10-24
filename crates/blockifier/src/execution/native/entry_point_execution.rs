use cairo_native::execution_result::ContractExecutionResult;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_traits::ToPrimitive;

use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::contract_class::{NativeContractClassV1, TrackedResource};
use crate::execution::entry_point::{
    CallEntryPoint,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
};
use crate::execution::errors::{EntryPointExecutionError, PostExecutionError};
use crate::execution::native::syscall_handler::NativeSyscallHandler;
use crate::state::state_api::State;

pub fn execute_entry_point_call(
    call: CallEntryPoint,
    contract_class: NativeContractClassV1,
    state: &mut dyn State,
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let function_id = contract_class.get_entry_point(&call)?;

    let mut syscall_handler: NativeSyscallHandler<'_> = NativeSyscallHandler::new(
        state,
        call.caller_address,
        call.storage_address,
        call.entry_point_selector,
        resources,
        context,
    );

    let execution_result = contract_class.executor.invoke_contract_dynamic(
        &function_id,
        &call.calldata.0,
        Some(call.initial_gas.into()),
        &mut syscall_handler,
    );

    let call_result = execution_result.map_err(EntryPointExecutionError::NativeUnexpectedError)?;
    create_call_info(call, call_result, syscall_handler)
}

fn create_call_info(
    call: CallEntryPoint,
    call_result: ContractExecutionResult,
    syscall_handler: NativeSyscallHandler<'_>,
) -> Result<CallInfo, EntryPointExecutionError> {
    let remaining_gas =
        call_result.remaining_gas.to_u64().ok_or(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas. Gas value is bigger than u64: {}",
                call_result.remaining_gas
            ),
        })?;
    if remaining_gas > call.initial_gas {
        return Err(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas. Used gas is greater than initial gas: {} > {}",
                remaining_gas, call.initial_gas
            ),
        }
        .into());
    }

    let gas_consumed = call.initial_gas - remaining_gas;

    // todo(rodrigo): execution resources for native execution are still wip until future
    // development on both Cairo lang and the Native compiler
    let versioned_constants = syscall_handler.context.versioned_constants();
    *syscall_handler.resources +=
        &versioned_constants.get_additional_os_syscall_resources(&syscall_handler.syscall_counter);

    Ok(CallInfo {
        call,
        execution: CallExecution {
            retdata: Retdata(call_result.return_values),
            events: syscall_handler.events,
            l2_to_l1_messages: syscall_handler.l2_to_l1_messages,
            failed: call_result.failure_flag,
            gas_consumed,
        },
        resources: syscall_handler.resources.clone(),
        inner_calls: syscall_handler.inner_calls,
        storage_read_values: syscall_handler.read_values,
        accessed_storage_keys: syscall_handler.accessed_keys,
        tracked_resource: TrackedResource::SierraGas,
    })
}
