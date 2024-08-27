use std::collections::{HashMap, HashSet};
use std::hash::RandomState;

use cairo_lang_sierra::ids::FunctionId;
use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotNativeExecutor;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use itertools::Itertools;
use num_traits::ToBytes;
use starknet_api::core::EntryPointSelector;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{
    CallExecution,
    CallInfo,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
};
use crate::execution::entry_point::{CallEntryPoint, EntryPointExecutionResult};
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::native::syscall_handler::NativeSyscallHandler;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod test;

// An arbitrary number, chosen to avoid accidentally aligning with actually calculated gas
// To be deleted once cairo native gas handling can be used.
pub const NATIVE_GAS_PLACEHOLDER: u64 = 12;

pub fn contract_entrypoint_to_entrypoint_selector(
    entrypoint: &ContractEntryPoint,
) -> EntryPointSelector {
    EntryPointSelector(Felt::from(&entrypoint.selector))
}

pub fn run_native_executor(
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
        Ok(res) if res.failure_flag => Err(EntryPointExecutionError::NativeExecutionError {
            info: if !res.return_values.is_empty() {
                decode_felts_as_str(&res.return_values)
            } else {
                String::from("Unknown error")
            },
        }),
        Err(runner_err) => {
            Err(EntryPointExecutionError::NativeUnexpectedError { source: runner_err })
        }
        Ok(res) => Ok(res),
    }?;

    Ok(create_callinfo(
        call.clone(),
        run_result,
        syscall_handler.events,
        syscall_handler.l2_to_l1_messages,
        syscall_handler.inner_calls,
        syscall_handler.storage_read_values,
        syscall_handler.accessed_storage_keys,
    ))
}

pub fn create_callinfo(
    call: CallEntryPoint,
    run_result: ContractExecutionResult,
    events: Vec<OrderedEvent>,
    l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    inner_calls: Vec<CallInfo>,
    storage_read_values: Vec<Felt>,
    accessed_storage_keys: HashSet<StorageKey, RandomState>,
) -> CallInfo {
    CallInfo {
        call,
        execution: CallExecution {
            retdata: Retdata(run_result.return_values),
            events,
            l2_to_l1_messages,
            failed: run_result.failure_flag,
            gas_consumed: NATIVE_GAS_PLACEHOLDER,
        },
        resources: ExecutionResources {
            n_steps: 0,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::default(),
        },
        inner_calls,
        storage_read_values,
        accessed_storage_keys,
    }
}

pub fn encode_str_as_felts(msg: &str) -> Vec<Felt> {
    const CHUNK_SIZE: usize = 32;

    let data = msg.as_bytes().chunks(CHUNK_SIZE - 1);
    let mut encoding = vec![Felt::default(); data.len()];
    for (i, data_chunk) in data.enumerate() {
        let mut chunk = [0_u8; CHUNK_SIZE];
        chunk[1..data_chunk.len() + 1].copy_from_slice(data_chunk);
        encoding[i] = Felt::from_bytes_be(&chunk);
    }
    encoding
}

pub fn decode_felts_as_str(encoding: &[Felt]) -> String {
    let bytes_err: Vec<_> =
        encoding.iter().flat_map(|felt| felt.to_bytes_be()[1..32].to_vec()).collect();

    match String::from_utf8(bytes_err) {
        Ok(s) => s.trim_matches('\0').to_owned(),
        Err(_) => {
            let err_msgs = encoding
                .iter()
                .map(|felt| match String::from_utf8(felt.to_bytes_be()[1..32].to_vec()) {
                    Ok(s) => format!("{} ({})", s.trim_matches('\0'), felt),
                    Err(_) => felt.to_string(),
                })
                .join(", ");
            format!("[{}]", err_msgs)
        }
    }
}
