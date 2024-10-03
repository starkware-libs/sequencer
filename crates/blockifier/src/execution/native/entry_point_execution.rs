use std::collections::HashMap;

use cairo_lang_sierra::ids::FunctionId;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotNativeExecutor;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use crate::execution::call_info::{CallExecution, CallInfo, Retdata};
use crate::execution::contract_class::{NativeContractClassV1, TrackedResource};
use crate::execution::entry_point::{
    CallEntryPoint,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
};
use crate::execution::errors::EntryPointExecutionError;
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

    let run_result = match execution_result {
        Err(runner_err) => {
            Err(EntryPointExecutionError::NativeUnexpectedError { source: runner_err })
        }
        Ok(res) if res.failure_flag => Err(EntryPointExecutionError::NativeExecutionError {
            info: if !res.return_values.is_empty() {
                decode_felts_as_str(&res.return_values)
            } else {
                String::from("Unknown error")
            },
        }),
        Ok(res) => Ok(res),
    }?;

    create_callinfo(call.clone(), run_result, syscall_handler)
}

fn create_callinfo(
    call: CallEntryPoint,
    run_result: ContractExecutionResult,
    syscall_handler: NativeSyscallHandler<'_>,
) -> Result<CallInfo, EntryPointExecutionError> {
    let gas_consumed = {
        // We can use `.unwrap()` directly in both cases because the most significant bit is could
        // be only 63 here (128 = 64 + 64).
        let low: u64 = (run_result.remaining_gas & ((1u128 << 64) - 1)).try_into().unwrap();
        let high: u64 = (run_result.remaining_gas >> 64).try_into().unwrap();
        if high != 0 {
            return Err(EntryPointExecutionError::NativeExecutionError {
                info: "Overflow: gas consumed bigger than 64 bit".into(),
            });
        }
        call.initial_gas - low
    };

    Ok(CallInfo {
        call,
        execution: CallExecution {
            retdata: Retdata(run_result.return_values),
            events: syscall_handler.events,
            l2_to_l1_messages: syscall_handler.l2_to_l1_messages,
            failed: run_result.failure_flag,
            gas_consumed,
        },
        // todo(rodrigo): execution resources rely heavily on how the VM work, therefore
        // the dummy values
        resources: ExecutionResources {
            n_steps: 0,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::default(),
        },
        inner_calls: syscall_handler.inner_calls,
        storage_read_values: syscall_handler.read_values,
        accessed_storage_keys: syscall_handler.accessed_keys,
        tracked_resource: TrackedResource::SierraGas,
    })
}
