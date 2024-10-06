use std::collections::{HashMap, HashSet};
use std::hash::RandomState;

use ark_ff::BigInt;
use cairo_lang_sierra::ids::FunctionId;
use cairo_lang_starknet_classes::contract_class::ContractEntryPoint;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotNativeExecutor;
use cairo_native::starknet::{ResourceBounds, SyscallResult, TxV2Info, U256};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use itertools::Itertools;
use num_bigint::BigUint;
use num_traits::ToBytes;
use starknet_api::core::{ContractAddress, EntryPointSelector};
use starknet_api::state::StorageKey;
use starknet_api::transaction::Resource;
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
use crate::execution::syscalls::hint_processor::{L1_GAS, L2_GAS};
use crate::transaction::objects::CurrentTransactionInfo;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod test;

// An arbitrary number, chosen to avoid accidentally aligning with actually calculated gas
// To be deleted once cairo native gas handling can be used.
pub const NATIVE_GAS_PLACEHOLDER: u64 = 12;

pub fn contract_address_to_native_felt(contract_address: ContractAddress) -> Felt {
    *contract_address.0.key()
}

pub fn contract_entrypoint_to_entrypoint_selector(
    entrypoint: &ContractEntryPoint,
) -> EntryPointSelector {
    let selector_felt = Felt::from_bytes_be_slice(&entrypoint.selector.to_be_bytes());
    EntryPointSelector(selector_felt)
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

    create_callinfo(
        call.clone(),
        run_result,
        syscall_handler.events,
        syscall_handler.l2_to_l1_messages,
        syscall_handler.inner_calls,
        syscall_handler.storage_read_values,
        syscall_handler.accessed_storage_keys,
    )
}

pub fn create_callinfo(
    call: CallEntryPoint,
    run_result: ContractExecutionResult,
    events: Vec<OrderedEvent>,
    l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    inner_calls: Vec<CallInfo>,
    storage_read_values: Vec<Felt>,
    accessed_storage_keys: HashSet<StorageKey, RandomState>,
) -> Result<CallInfo, EntryPointExecutionError> {
    Ok(CallInfo {
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
    })
}

pub fn u256_to_biguint(u256: U256) -> BigUint {
    let lo = BigUint::from(u256.lo);
    let hi = BigUint::from(u256.hi);

    (hi << 128) + lo
}

pub fn big4int_to_u256(b_int: BigInt<4>) -> U256 {
    let [a, b, c, d] = b_int.0;

    let lo = u128::from(a) | (u128::from(b) << 64);
    let hi = u128::from(c) | (u128::from(d) << 64);

    U256 { lo, hi }
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

pub fn default_tx_v2_info() -> TxV2Info {
    TxV2Info {
        version: Default::default(),
        account_contract_address: Default::default(),
        max_fee: 0,
        signature: vec![],
        transaction_hash: Default::default(),
        chain_id: Default::default(),
        nonce: Default::default(),
        resource_bounds: vec![],
        tip: 0,
        paymaster_data: vec![],
        nonce_data_availability_mode: 0,
        fee_data_availability_mode: 0,
        account_deployment_data: vec![],
    }
}

pub fn calculate_resource_bounds(
    tx_info: &CurrentTransactionInfo,
) -> SyscallResult<Vec<ResourceBounds>> {
    let l1_gas = Felt::from_hex(L1_GAS).map_err(|e| encode_str_as_felts(&e.to_string()))?;
    let l2_gas = Felt::from_hex(L2_GAS).map_err(|e| encode_str_as_felts(&e.to_string()))?;

    Ok(tx_info
        .resource_bounds
        .0
        .iter()
        .map(|(resource, resource_bound)| {
            let resource = match resource {
                Resource::L1Gas => l1_gas,
                Resource::L2Gas => l2_gas,
            };

            ResourceBounds {
                resource,
                max_amount: resource_bound.max_amount,
                max_price_per_unit: resource_bound.max_price_per_unit,
            }
        })
        .collect())
}
