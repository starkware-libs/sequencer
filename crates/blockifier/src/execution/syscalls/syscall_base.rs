use std::collections::{HashMap, HashSet};
use std::convert::From;
use std::hash::RandomState;

use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::{CallEntryPoint, EntryPointExecutionContext};
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
};
use crate::state::state_api::State;

pub type SyscallResult<T> = Result<T, SyscallExecutionError>;

/// This file is for sharing common logic between Native and VM syscall implementations.

pub struct SyscallHandlerBase<'state> {
    // Input for execution.
    pub state: &'state mut dyn State,
    pub context: &'state mut EntryPointExecutionContext,
    pub call: CallEntryPoint,

    // Execution results.
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub inner_calls: Vec<CallInfo>,

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey, RandomState>,
    pub read_class_hash_values: Vec<ClassHash>,
    // Accessed addresses by the `get_class_hash_at` syscall.
    pub accessed_contract_addresses: HashSet<ContractAddress>,

    // The original storage value of the executed contract.
    // Should be moved back `context.revert_info` before executing an inner call.
    pub original_values: HashMap<StorageKey, Felt>,
}

impl<'state> SyscallHandlerBase<'state> {
    pub fn new(
        call: CallEntryPoint,
        state: &'state mut dyn State,
        context: &'state mut EntryPointExecutionContext,
    ) -> SyscallHandlerBase<'state> {
        let original_values = std::mem::take(
            &mut context
                .revert_infos
                .0
                .last_mut()
                .expect("Missing contract revert info.")
                .original_values,
        );
        SyscallHandlerBase {
            state,
            call,
            context,
            events: Vec::new(),
            l2_to_l1_messages: Vec::new(),
            inner_calls: Vec::new(),
            read_values: Vec::new(),
            accessed_keys: HashSet::new(),
            read_class_hash_values: Vec::new(),
            accessed_contract_addresses: HashSet::new(),
            original_values,
        }
    }
}

pub fn get_block_hash_base(
    context: &EntryPointExecutionContext,
    requested_block_number: u64,
    state: &dyn State,
) -> SyscallResult<Felt> {
    let execution_mode = context.execution_mode;
    if execution_mode == ExecutionMode::Validate {
        return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
            syscall_name: "get_block_hash".to_string(),
            execution_mode,
        });
    }

    let current_block_number = context.tx_context.block_context.block_info.block_number.0;

    if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
        || requested_block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
    {
        let out_of_range_error = Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR)
            .expect("Converting BLOCK_NUMBER_OUT_OF_RANGE_ERROR to Felt should not fail.");
        return Err(SyscallExecutionError::SyscallError { error_data: vec![out_of_range_error] });
    }

    let key = StorageKey::try_from(Felt::from(requested_block_number))?;
    let block_hash_contract_address =
        ContractAddress::try_from(Felt::from(constants::BLOCK_HASH_CONTRACT_ADDRESS))?;
    Ok(state.get_storage_at(block_hash_contract_address, key)?)
}
