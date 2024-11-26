use std::collections::{hash_map, HashMap, HashSet};
use std::convert::From;

use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::EventContent;
use starknet_types_core::felt::Felt;

use super::exceeds_event_size_limit;
use crate::abi::constants;
use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::{CallEntryPoint, EntryPointExecutionContext};
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
    ENTRYPOINT_FAILED_ERROR,
};
use crate::state::state_api::State;
use crate::transaction::account_transaction::is_cairo1;

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
    pub accessed_keys: HashSet<StorageKey>,
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

    pub fn get_block_hash(&self, requested_block_number: u64) -> SyscallResult<Felt> {
        let execution_mode = self.context.execution_mode;
        if execution_mode == ExecutionMode::Validate {
            return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "get_block_hash".to_string(),
                execution_mode,
            });
        }

        let current_block_number = self.context.tx_context.block_context.block_info.block_number.0;

        if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
            || requested_block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
        {
            let out_of_range_error = Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR)
                .expect("Converting BLOCK_NUMBER_OUT_OF_RANGE_ERROR to Felt should not fail.");
            return Err(SyscallExecutionError::SyscallError {
                error_data: vec![out_of_range_error],
            });
        }

        let key = StorageKey::try_from(Felt::from(requested_block_number))?;
        let block_hash_contract_address =
            ContractAddress::try_from(Felt::from(constants::BLOCK_HASH_CONTRACT_ADDRESS))?;
        Ok(self.state.get_storage_at(block_hash_contract_address, key)?)
    }

    pub fn storage_read(&mut self, key: StorageKey) -> SyscallResult<Felt> {
        self.accessed_keys.insert(key);
        let value = self.state.get_storage_at(self.call.storage_address, key)?;
        self.read_values.push(value);
        Ok(value)
    }

    pub fn storage_write(&mut self, key: StorageKey, value: Felt) -> SyscallResult<()> {
        let contract_address = self.call.storage_address;

        match self.original_values.entry(key) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(self.state.get_storage_at(contract_address, key)?);
            }
            hash_map::Entry::Occupied(_) => {}
        }

        self.accessed_keys.insert(key);
        self.state.set_storage_at(contract_address, key, value)?;

        Ok(())
    }

    pub fn get_class_hash_at(
        &mut self,
        contract_address: ContractAddress,
    ) -> SyscallResult<ClassHash> {
        self.accessed_contract_addresses.insert(contract_address);
        let class_hash = self.state.get_class_hash_at(contract_address)?;
        self.read_class_hash_values.push(class_hash);
        Ok(class_hash)
    }

    pub fn emit_event(&mut self, event: EventContent) -> SyscallResult<()> {
        exceeds_event_size_limit(
            self.context.versioned_constants(),
            self.context.n_emitted_events + 1,
            &event,
        )?;
        let ordered_event = OrderedEvent { order: self.context.n_emitted_events, event };
        self.events.push(ordered_event);
        self.context.n_emitted_events += 1;

        Ok(())
    }

    pub fn replace_class(&mut self, class_hash: ClassHash) -> SyscallResult<()> {
        // Ensure the class is declared (by reading it), and of type V1.
        let class = self.state.get_compiled_contract_class(class_hash)?;

        if !is_cairo1(&class) {
            return Err(SyscallExecutionError::ForbiddenClassReplacement { class_hash });
        }
        self.state.set_class_hash_at(self.call.storage_address, class_hash)?;
        Ok(())
    }

    pub fn execute_inner_call(
        &mut self,
        call: CallEntryPoint,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        let revert_idx = self.context.revert_infos.0.len();

        let call_info = call.execute(self.state, self.context, remaining_gas)?;

        let mut raw_retdata = call_info.execution.retdata.0.clone();
        let failed = call_info.execution.failed;
        self.inner_calls.push(call_info);
        if failed {
            self.context.revert(revert_idx, self.state)?;

            // Delete events and l2_to_l1_messages from the reverted call.
            let reverted_call = &mut self.inner_calls.last_mut().unwrap();
            let mut stack: Vec<&mut CallInfo> = vec![reverted_call];
            while let Some(call_info) = stack.pop() {
                call_info.execution.events.clear();
                call_info.execution.l2_to_l1_messages.clear();
                // Add inner calls that did not fail to the stack.
                // The events and l2_to_l1_messages of the failed calls were already cleared.
                stack.extend(
                    call_info
                        .inner_calls
                        .iter_mut()
                        .filter(|call_info| !call_info.execution.failed),
                );
            }

            raw_retdata.push(
                Felt::from_hex(ENTRYPOINT_FAILED_ERROR).map_err(SyscallExecutionError::from)?,
            );
            return Err(SyscallExecutionError::SyscallError { error_data: raw_retdata });
        }

        Ok(raw_retdata)
    }

    pub fn finalize(&mut self) {
        self.context
            .revert_infos
            .0
            .last_mut()
            .expect("Missing contract revert info.")
            .original_values = std::mem::take(&mut self.original_values);
    }
}
