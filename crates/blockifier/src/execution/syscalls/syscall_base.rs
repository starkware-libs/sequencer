/// This file is for sharing common logic between Native and VM syscall implementations.
use std::collections::{hash_map, HashMap};
use std::convert::From;

use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::EventContent;
use starknet_types_core::felt::Felt;

use super::exceeds_event_size_limit;
use crate::abi::constants;
use crate::execution::call_info::{
    CallInfo,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    StorageAccessTracker,
};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::{
    CallEntryPoint,
    ConstructorContext,
    EntryPointExecutionContext,
    ExecutableCallEntryPoint,
};
use crate::execution::execution_utils::execute_deployment;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
    ENTRYPOINT_FAILED_ERROR,
    INVALID_INPUT_LENGTH_ERROR,
    OUT_OF_GAS_ERROR,
};
use crate::state::state_api::State;
use crate::transaction::account_transaction::is_cairo1;

pub type SyscallResult<T> = Result<T, SyscallExecutionError>;
pub const KECCAK_FULL_RATE_IN_WORDS: usize = 17;

pub struct SyscallHandlerBase<'state> {
    // Input for execution.
    pub state: &'state mut dyn State,
    pub context: &'state mut EntryPointExecutionContext,
    pub call: ExecutableCallEntryPoint,

    // Execution results.
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub inner_calls: Vec<CallInfo>,

    // Additional information gathered during execution.
    pub storage_access_tracker: StorageAccessTracker,

    // The original storage value of the executed contract.
    // Should be moved back `context.revert_info` before executing an inner call.
    pub original_values: HashMap<StorageKey, Felt>,

    revert_info_idx: usize,
}

impl<'state> SyscallHandlerBase<'state> {
    pub fn new(
        call: ExecutableCallEntryPoint,
        state: &'state mut dyn State,
        context: &'state mut EntryPointExecutionContext,
    ) -> SyscallHandlerBase<'state> {
        let revert_info_idx = context.revert_infos.0.len() - 1;
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
            storage_access_tracker: StorageAccessTracker::default(),
            original_values,
            revert_info_idx,
        }
    }

    pub fn get_block_hash(&mut self, requested_block_number: u64) -> SyscallResult<Felt> {
        // Note: we take the actual block number (and not the rounded one for validate)
        // in any case; it is consistent with the OS implementation and safe (see `Validate` arm).
        let current_block_number = self.context.tx_context.block_context.block_info.block_number.0;

        if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
            || requested_block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
        {
            // Requested block is too recent.
            match self.context.execution_mode {
                ExecutionMode::Execute => {
                    // Revert the syscall.
                    let out_of_range_error = Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR)
                        .expect(
                            "Converting BLOCK_NUMBER_OUT_OF_RANGE_ERROR to Felt should not fail.",
                        );
                    return Err(SyscallExecutionError::Revert {
                        error_data: vec![out_of_range_error],
                    });
                }
                ExecutionMode::Validate => {
                    // In this case, the transaction must be **rejected** to avoid the following
                    // attack:
                    //   * query a given block in validate,
                    //   * if reverted - ignore, if succeeded - panic.
                    //   * in the gateway, the queried block is (actual_latest - 9),
                    //   * while in the sequencer, the queried block can be further than that.
                    return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
                        syscall_name: "get_block_hash on recent blocks".to_string(),
                        execution_mode: ExecutionMode::Validate,
                    });
                }
            }
        }

        self.storage_access_tracker.accessed_blocks.insert(BlockNumber(requested_block_number));
        let key = StorageKey::try_from(Felt::from(requested_block_number))?;
        let block_hash_contract_address = self
            .context
            .tx_context
            .block_context
            .versioned_constants
            .os_constants
            .os_contract_addresses
            .block_hash_contract_address();
        let block_hash = self.state.get_storage_at(block_hash_contract_address, key)?;
        self.storage_access_tracker.read_block_hash_values.push(BlockHash(block_hash));
        Ok(block_hash)
    }

    pub fn storage_read(&mut self, key: StorageKey) -> SyscallResult<Felt> {
        self.storage_access_tracker.accessed_storage_keys.insert(key);
        let value = self.state.get_storage_at(self.call.storage_address, key)?;
        self.storage_access_tracker.storage_read_values.push(value);
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

        self.storage_access_tracker.accessed_storage_keys.insert(key);
        self.state.set_storage_at(contract_address, key, value)?;

        Ok(())
    }

    pub fn get_class_hash_at(
        &mut self,
        contract_address: ContractAddress,
    ) -> SyscallResult<ClassHash> {
        if self.context.execution_mode == ExecutionMode::Validate {
            return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "get_class_hash_at".to_string(),
                execution_mode: ExecutionMode::Validate,
            });
        }
        self.storage_access_tracker.accessed_contract_addresses.insert(contract_address);
        let class_hash = self.state.get_class_hash_at(contract_address)?;
        self.storage_access_tracker.read_class_hash_values.push(class_hash);
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
        let compiled_class = self.state.get_compiled_class(class_hash)?;

        if !is_cairo1(&compiled_class) {
            return Err(SyscallExecutionError::ForbiddenClassReplacement { class_hash });
        }
        self.state.set_class_hash_at(self.call.storage_address, class_hash)?;
        Ok(())
    }

    pub fn deploy(
        &mut self,
        class_hash: ClassHash,
        contract_address_salt: ContractAddressSalt,
        constructor_calldata: Calldata,
        deploy_from_zero: bool,
        remaining_gas: &mut u64,
    ) -> SyscallResult<(ContractAddress, CallInfo)> {
        let deployer_address = self.call.storage_address;
        let deployer_address_for_calculation = match deploy_from_zero {
            true => ContractAddress::default(),
            false => deployer_address,
        };
        let deployed_contract_address = calculate_contract_address(
            contract_address_salt,
            class_hash,
            &constructor_calldata,
            deployer_address_for_calculation,
        )?;

        let ctor_context = ConstructorContext {
            class_hash,
            code_address: Some(deployed_contract_address),
            storage_address: deployed_contract_address,
            caller_address: deployer_address,
        };
        let call_info = execute_deployment(
            self.state,
            self.context,
            ctor_context,
            constructor_calldata,
            remaining_gas,
        )?;
        Ok((deployed_contract_address, call_info))
    }

    pub fn send_message_to_l1(&mut self, message: MessageToL1) -> SyscallResult<()> {
        let ordered_message_to_l1 =
            OrderedL2ToL1Message { order: self.context.n_sent_messages_to_l1, message };
        self.l2_to_l1_messages.push(ordered_message_to_l1);
        self.context.n_sent_messages_to_l1 += 1;

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
            return Err(SyscallExecutionError::Revert { error_data: raw_retdata });
        }

        Ok(raw_retdata)
    }

    pub fn keccak(
        &mut self,
        input: &[u64],
        remaining_gas: &mut u64,
    ) -> SyscallResult<([u64; 4], usize)> {
        let input_length = input.len();

        let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

        if remainder != 0 {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![
                    Felt::from_hex(INVALID_INPUT_LENGTH_ERROR)
                        .expect("Failed to parse INVALID_INPUT_LENGTH_ERROR hex string"),
                ],
            });
        }
        // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
        // works.
        let n_rounds_as_u64 = u64::try_from(n_rounds).expect("Failed to convert usize to u64.");
        let gas_cost = n_rounds_as_u64 * self.context.gas_costs().syscalls.keccak_round_cost;

        if gas_cost > *remaining_gas {
            let out_of_gas_error = Felt::from_hex(OUT_OF_GAS_ERROR)
                .expect("Failed to parse OUT_OF_GAS_ERROR hex string");

            return Err(SyscallExecutionError::Revert { error_data: vec![out_of_gas_error] });
        }
        *remaining_gas -= gas_cost;

        let mut state = [0u64; 25];
        for chunk in input.chunks(KECCAK_FULL_RATE_IN_WORDS) {
            for (i, val) in chunk.iter().enumerate() {
                state[i] ^= val;
            }
            keccak::f1600(&mut state)
        }

        Ok((state[..4].try_into().expect("Slice with incorrect length"), n_rounds))
    }

    pub fn finalize(&mut self) {
        self.context
            .revert_infos
            .0
            .get_mut(self.revert_info_idx)
            .expect("Missing contract revert info.")
            .original_values = std::mem::take(&mut self.original_values);
    }
}
