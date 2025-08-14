/// This file is for sharing common logic between Native and VM syscall implementations.
use std::collections::{hash_map, HashMap};
use std::convert::From;
use std::sync::Arc;

use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use starknet_api::state::StorageKey;
use starknet_api::transaction::constants::EXECUTE_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, Fee, TransactionSignature};
use starknet_api::transaction::{
    signed_tx_version,
    EventContent,
    InvokeTransactionV0,
    TransactionHasher,
    TransactionOptions,
    TransactionVersion,
};
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::context::TransactionContext;
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
    CallType,
    ConstructorContext,
    EntryPointExecutionContext,
    ExecutableCallEntryPoint,
};
use crate::execution::execution_utils::execute_deployment;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
    ENTRYPOINT_FAILED_ERROR,
    INVALID_ARGUMENT,
};
use crate::execution::syscalls::vm_syscall_utils::{
    exceeds_event_size_limit,
    SyscallBaseResult,
    SyscallExecutorBaseError,
    SyscallSelector,
    SyscallUsageMap,
    TryExtractRevert,
};
use crate::state::state_api::State;
use crate::transaction::account_transaction::is_cairo1;
use crate::transaction::objects::{
    CommonAccountFields,
    DeprecatedTransactionInfo,
    TransactionInfo,
};

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

    pub syscalls_usage: SyscallUsageMap,

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
            syscalls_usage: SyscallUsageMap::new(),
            revert_info_idx,
        }
    }

    pub fn increment_syscall_count_by(&mut self, selector: SyscallSelector, n: usize) {
        let syscall_usage = self.syscalls_usage.entry(selector).or_default();
        syscall_usage.call_count += n;
    }

    pub fn increment_syscall_linear_factor_by(&mut self, selector: &SyscallSelector, n: usize) {
        let syscall_usage = self
            .syscalls_usage
            .get_mut(selector)
            .expect("syscalls_usage entry must be initialized before incrementing linear factor");
        syscall_usage.linear_factor += n;
    }

    #[allow(clippy::result_large_err)]
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
                    self.reject_syscall_in_validate_mode("get_block_hash on recent blocks")?;
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
            self.reject_syscall_in_validate_mode("get_class_hash_at")?;
        }

        self.storage_access_tracker.accessed_contract_addresses.insert(contract_address);
        let class_hash = self.state.get_class_hash_at(contract_address)?;
        self.storage_access_tracker.read_class_hash_values.push(class_hash);
        Ok(class_hash)
    }

    /// Returns the transaction version for the `get_execution_info` syscall.
    pub fn tx_version_for_get_execution_info(&self) -> TransactionVersion {
        let tx_context = &self.context.tx_context;
        // The transaction version, ignoring the only_query bit.
        let version = tx_context.tx_info.version();
        let versioned_constants = &tx_context.block_context.versioned_constants;
        // The set of v1-bound-accounts.
        let v1_bound_accounts = &versioned_constants.os_constants.v1_bound_accounts_cairo1;
        let class_hash = &self.call.class_hash;

        // If the transaction version is 3 and the account is in the v1-bound-accounts set,
        // the syscall should return transaction version 1 instead.
        if version == TransactionVersion::THREE && v1_bound_accounts.contains(class_hash) {
            let tip = match &tx_context.tx_info {
                TransactionInfo::Current(transaction_info) => transaction_info.tip,
                TransactionInfo::Deprecated(_) => {
                    panic!("Transaction info variant doesn't match transaction version")
                }
            };
            if tip <= versioned_constants.os_constants.v1_bound_accounts_max_tip {
                return signed_tx_version(
                    &TransactionVersion::ONE,
                    &TransactionOptions { only_query: tx_context.tx_info.only_query() },
                );
            }
        }

        tx_context.tx_info.signed_version()
    }

    /// Return whether the L1 data gas should be excluded for the `get_execution_info` syscall.
    pub fn should_exclude_l1_data_gas(&self) -> bool {
        let class_hash = self.call.class_hash;
        let versioned_constants = &self.context.tx_context.block_context.versioned_constants;
        versioned_constants.os_constants.data_gas_accounts.contains(&class_hash)
            && self.context.tx_context.tx_info.version() == TransactionVersion::THREE
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

    pub fn meta_tx_v0(
        &mut self,
        contract_address: ContractAddress,
        entry_point_selector: EntryPointSelector,
        calldata: Calldata,
        signature: TransactionSignature,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        self.increment_syscall_linear_factor_by(&SyscallSelector::MetaTxV0, calldata.0.len());
        if self.context.execution_mode == ExecutionMode::Validate {
            self.reject_syscall_in_validate_mode("meta_tx_v0")?;
        }
        if entry_point_selector != selector_from_name(EXECUTE_ENTRY_POINT_NAME) {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![Felt::from_hex(INVALID_ARGUMENT).unwrap()],
            });
        }
        let entry_point = CallEntryPoint {
            class_hash: None,
            code_address: Some(contract_address),
            entry_point_type: EntryPointType::External,
            entry_point_selector,
            calldata: calldata.clone(),
            storage_address: contract_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            // NOTE: this value might be overridden later on.
            initial_gas: *remaining_gas,
        };

        let old_tx_context = self.context.tx_context.clone();
        let only_query = old_tx_context.tx_info.only_query();

        // Compute meta-transaction hash.
        let transaction_hash = InvokeTransactionV0 {
            max_fee: Fee(0),
            signature: signature.clone(),
            contract_address,
            entry_point_selector,
            calldata,
        }
        .calculate_transaction_hash(
            &self.context.tx_context.block_context.chain_info.chain_id,
            &signed_tx_version(&TransactionVersion::ZERO, &TransactionOptions { only_query }),
        )?;

        let class_hash = self.state.get_class_hash_at(contract_address)?;

        // Replace `tx_context`.
        let new_tx_info = TransactionInfo::Deprecated(DeprecatedTransactionInfo {
            common_fields: CommonAccountFields {
                transaction_hash,
                version: TransactionVersion::ZERO,
                signature,
                nonce: Nonce(0.into()),
                sender_address: contract_address,
                only_query,
            },
            max_fee: Fee(0),
        });
        self.context.tx_context = Arc::new(TransactionContext {
            block_context: old_tx_context.block_context.clone(),
            tx_info: new_tx_info,
        });

        // No error should be propagated until we restore the old `tx_context`.
        let result = self.execute_inner_call(entry_point, remaining_gas).map_err(|error| {
            SyscallExecutionError::from_self_or_revert(error.try_extract_revert().map_original(
                |error| {
                    // TODO(lior): Change to meta-tx specific error.
                    error.as_call_contract_execution_error(
                        class_hash,
                        contract_address,
                        entry_point_selector,
                    )
                },
            ))
        });

        // Restore the old `tx_context`.
        self.context.tx_context = old_tx_context;

        result
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
        self.increment_syscall_linear_factor_by(
            &SyscallSelector::Deploy,
            constructor_calldata.0.len(),
        );
        let versioned_constants = &self.context.tx_context.block_context.versioned_constants;
        if should_reject_deploy(
            versioned_constants.disable_deploy_in_validation_mode,
            self.context.execution_mode,
        ) {
            self.reject_syscall_in_validate_mode("deploy")?;
        }

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
        if !self.context.tx_context.block_context.chain_info.is_l3 {
            EthAddress::try_from(message.to_address)?;
        }
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

    pub fn finalize(&mut self) {
        self.context
            .revert_infos
            .0
            .get_mut(self.revert_info_idx)
            .expect("Missing contract revert info.")
            .original_values = std::mem::take(&mut self.original_values);
    }

    pub(crate) fn maybe_block_direct_execute_call(
        &mut self,
        selector: EntryPointSelector,
    ) -> SyscallResult<()> {
        let versioned_constants = &self.context.tx_context.block_context.versioned_constants;
        if versioned_constants.block_direct_execute_call
            && selector == selector_from_name(EXECUTE_ENTRY_POINT_NAME)
        {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![Felt::from_hex(INVALID_ARGUMENT).unwrap()],
            });
        }
        Ok(())
    }

    fn reject_syscall_in_validate_mode(&self, syscall_name: &str) -> SyscallBaseResult<()> {
        Err(SyscallExecutorBaseError::InvalidSyscallInExecutionMode {
            syscall_name: syscall_name.to_string(),
            execution_mode: ExecutionMode::Validate,
        })
    }
}

pub(crate) fn should_reject_deploy(
    disable_deploy_in_validation_mode: bool,
    execution_mode: ExecutionMode,
) -> bool {
    disable_deploy_in_validation_mode && execution_mode == ExecutionMode::Validate
}
