use cairo_lang_sierra::ids::FunctionId;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotNativeExecutor;
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
use crate::execution::native::utils::decode_felts_as_str;
use crate::state::state_api::State;

pub fn execute_entry_point_call(
    call: CallEntryPoint,
    contract_class: NativeContractClassV1,
    state: &mut dyn State,
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let function_id = contract_class.get_entry_point(&call)?;

    let syscall_handler: NativeSyscallHandler<'_> = NativeSyscallHandler::new(
        state,
        call.caller_address,
        call.storage_address,
        call.entry_point_selector,
        resources,
        context,
    );

    run_native_executor(&contract_class.executor, function_id, call, syscall_handler)
}

fn run_native_executor(
    native_executor: &AotNativeExecutor,
    function_id: &FunctionId,
    call: CallEntryPoint,
    mut syscall_handler: NativeSyscallHandler<'_>,
) -> EntryPointExecutionResult<CallInfo> {
    let execution_result = native_executor.invoke_contract_dynamic(
        function_id,
        &call.calldata.0,
        Some(call.initial_gas.into()),
        &mut syscall_handler,
    );

    let call_result = match execution_result {
        Err(runner_err) => Err(EntryPointExecutionError::NativeUnexpectedError(runner_err)),
        Ok(res) if res.failure_flag => Err(EntryPointExecutionError::NativeExecutionError {
            info: if !res.return_values.is_empty() {
                decode_felts_as_str(&res.return_values)
            } else {
                String::from("Unknown error")
            },
        }),
        Ok(res) => Ok(res),
    }?;

    create_callinfo(call, call_result, syscall_handler)
}

fn create_callinfo(
    call: CallEntryPoint,
    call_result: ContractExecutionResult,
    syscall_handler: NativeSyscallHandler<'_>,
) -> Result<CallInfo, EntryPointExecutionError> {
    // todo(rodro): Even if the property is called `remaining_gas` it behaves like gas used.
    // Update once gas works on Native side has been completed (or at least this part)
    let gas_used =
        call_result.remaining_gas.to_u64().ok_or(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas (bigger than u64): {}",
                call_result.remaining_gas
            ),
        })?;

    let gas_consumed = call.initial_gas - gas_used;

    Ok(CallInfo {
        call,
        execution: CallExecution {
            retdata: Retdata(call_result.return_values),
            events: syscall_handler.events,
            l2_to_l1_messages: syscall_handler.l2_to_l1_messages,
            failed: call_result.failure_flag,
            gas_consumed,
        },
        // todo(rodrigo): execution resources rely heavily on how the VM work, therefore
        // the dummy values
        resources: ExecutionResources::default(),
        inner_calls: syscall_handler.inner_calls,
        storage_read_values: syscall_handler.read_values,
        accessed_storage_keys: syscall_handler.accessed_keys,
        tracked_resource: TrackedResource::SierraGas,
    })
}
