use std::any::Any;
use std::collections::{HashMap, HashSet};

use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::{
    BuiltinHintProcessor,
    HintProcessorData,
};
use cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::cairo_runner::{ResourceTracker, RunResources};
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::{BigUint, TryFromBigIntError};
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockInfo, BlockNumber, BlockTimestamp};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
};
use starknet_api::state::StorageKey;
use starknet_api::transaction::constants::EXECUTE_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::Calldata;
use starknet_api::transaction::{signed_tx_version, TransactionOptions, TransactionVersion};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::{Felt, FromStrError};
use thiserror::Error;

use crate::context::TransactionContext;
use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message};
use crate::execution::common_hints::{
    extended_builtin_hint_processor,
    ExecutionMode,
    HintExecutionResult,
};
use crate::execution::deprecated_syscalls::deprecated_syscall_executor::{
    execute_next_deprecated_syscall,
    DeprecatedSyscallExecutor,
    DeprecatedSyscallExecutorBaseError,
    DeprecatedSyscallExecutorBaseResult,
};
use crate::execution::deprecated_syscalls::{
    CallContractRequest,
    CallContractResponse,
    DelegateCallRequest,
    DelegateCallResponse,
    DeployRequest,
    DeployResponse,
    DeprecatedSyscallResult,
    DeprecatedSyscallSelector,
    EmitEventRequest,
    EmitEventResponse,
    GetBlockNumberRequest,
    GetBlockNumberResponse,
    GetBlockTimestampRequest,
    GetBlockTimestampResponse,
    GetCallerAddressRequest,
    GetCallerAddressResponse,
    GetContractAddressRequest,
    GetContractAddressResponse,
    GetSequencerAddressRequest,
    GetSequencerAddressResponse,
    GetTxInfoRequest,
    GetTxInfoResponse,
    GetTxSignatureRequest,
    GetTxSignatureResponse,
    LibraryCallRequest,
    LibraryCallResponse,
    ReplaceClassRequest,
    ReplaceClassResponse,
    SendMessageToL1Request,
    SendMessageToL1Response,
    StorageReadRequest,
    StorageReadResponse,
    StorageWriteRequest,
    StorageWriteResponse,
};
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    ConstructorContext,
    EntryPointExecutionContext,
};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::execution_utils::{
    execute_deployment,
    felt_from_ptr,
    felt_range_from_ptr,
    ReadOnlySegment,
    ReadOnlySegments,
};
use crate::execution::hint_code;
use crate::execution::syscalls::hint_processor::EmitEventError;
use crate::execution::syscalls::syscall_base::should_reject_deploy;
use crate::execution::syscalls::vm_syscall_utils::{exceeds_event_size_limit, SyscallUsageMap};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::transaction::objects::TransactionInfo;

#[derive(Debug, Error)]
pub enum DeprecatedSyscallExecutionError {
    #[error("Bad syscall selector; expected: {expected_selector:?}, got: {actual_selector:?}.")]
    BadSyscallSelector {
        expected_selector: DeprecatedSyscallSelector,
        actual_selector: DeprecatedSyscallSelector,
    },
    #[error(transparent)]
    BaseError(#[from] DeprecatedSyscallExecutorBaseError),
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    ConstructorEntryPointExecutionError(#[from] ConstructorEntryPointExecutionError),
    #[error("{error}")]
    CallContractExecutionError {
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
        error: Box<DeprecatedSyscallExecutionError>,
    },
    #[error("Calling `__execute__` directly is not allowed.")]
    DirectExecuteCall,
    #[error(transparent)]
    EmitEventError(#[from] EmitEventError),
    #[error(transparent)]
    FromBigUint(#[from] TryFromBigIntError<BigUint>),
    #[error(transparent)]
    FromStr(#[from] FromStrError),
    #[error("{error}")]
    LibraryCallExecutionError {
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
        error: Box<DeprecatedSyscallExecutionError>,
    },
    #[error("Invalid syscall input: {input:?}; {info}")]
    InvalidSyscallInput { input: Felt, info: String },
    #[error(transparent)]
    MathError(#[from] cairo_vm::types::errors::math_errors::MathError),
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    VirtualMachineError(#[from] VirtualMachineError),
    #[error("Unauthorized syscall {syscall_name} in execution mode {execution_mode}.")]
    InvalidSyscallInExecutionMode { syscall_name: String, execution_mode: ExecutionMode },
}

// Needed for custom hint implementations (in our case, syscall hints) which must comply with the
// cairo-rs API.
impl From<DeprecatedSyscallExecutionError> for HintError {
    fn from(error: DeprecatedSyscallExecutionError) -> Self {
        HintError::Internal(VirtualMachineError::Other(error.into()))
    }
}

impl DeprecatedSyscallExecutionError {
    pub fn as_call_contract_execution_error(
        self,
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
    ) -> Self {
        DeprecatedSyscallExecutionError::CallContractExecutionError {
            class_hash,
            storage_address,
            selector,
            error: Box::new(self),
        }
    }

    pub fn as_lib_call_execution_error(
        self,
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
    ) -> Self {
        DeprecatedSyscallExecutionError::LibraryCallExecutionError {
            class_hash,
            storage_address,
            selector,
            error: Box::new(self),
        }
    }
}

/// Executes Starknet syscalls (stateful protocol hints) during the execution of an entry point
/// call.
pub struct DeprecatedSyscallHintProcessor<'a> {
    // Input for execution.
    pub state: &'a mut dyn State,
    pub context: &'a mut EntryPointExecutionContext,
    pub storage_address: ContractAddress,
    pub caller_address: ContractAddress,
    pub class_hash: ClassHash,

    // Execution results.
    /// Inner calls invoked by the current execution.
    pub inner_calls: Vec<CallInfo>,
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub syscalls_usage: SyscallUsageMap,

    // Fields needed for execution and validation.
    pub read_only_segments: ReadOnlySegments,
    pub syscall_ptr: Relocatable,

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey>,

    // Additional fields.
    // Invariant: must only contain allowed hints.
    builtin_hint_processor: BuiltinHintProcessor,
    // Transaction info. and signature segments; allocated on-demand.
    tx_signature_start_ptr: Option<Relocatable>,
    tx_info_start_ptr: Option<Relocatable>,
}

impl<'a> DeprecatedSyscallHintProcessor<'a> {
    pub fn new(
        state: &'a mut dyn State,
        context: &'a mut EntryPointExecutionContext,
        initial_syscall_ptr: Relocatable,
        storage_address: ContractAddress,
        caller_address: ContractAddress,
        class_hash: ClassHash,
    ) -> Self {
        DeprecatedSyscallHintProcessor {
            state,
            context,
            storage_address,
            caller_address,
            class_hash,
            inner_calls: vec![],
            events: vec![],
            l2_to_l1_messages: vec![],
            syscalls_usage: SyscallUsageMap::default(),
            read_only_segments: ReadOnlySegments::default(),
            syscall_ptr: initial_syscall_ptr,
            read_values: vec![],
            accessed_keys: HashSet::new(),
            builtin_hint_processor: extended_builtin_hint_processor(),
            tx_signature_start_ptr: None,
            tx_info_start_ptr: None,
        }
    }

    pub fn execution_mode(&self) -> ExecutionMode {
        self.context.execution_mode
    }

    pub fn is_validate_mode(&self) -> bool {
        self.execution_mode() == ExecutionMode::Validate
    }

    /// Returns an error if the syscall is run in validate mode.
    pub fn verify_not_in_validate_mode(&self, syscall_name: &str) -> DeprecatedSyscallResult<()> {
        if self.is_validate_mode() {
            return Err(DeprecatedSyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: syscall_name.to_string(),
                execution_mode: self.execution_mode(),
            });
        }

        Ok(())
    }

    pub fn get_or_allocate_tx_signature_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> DeprecatedSyscallResult<Relocatable> {
        match self.tx_signature_start_ptr {
            Some(tx_signature_start_ptr) => Ok(tx_signature_start_ptr),
            None => {
                let tx_signature_start_ptr = self.allocate_tx_signature_segment(vm)?;
                self.tx_signature_start_ptr = Some(tx_signature_start_ptr);
                Ok(tx_signature_start_ptr)
            }
        }
    }

    pub fn get_or_allocate_tx_info_start_ptr(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> DeprecatedSyscallResult<Relocatable> {
        let tx_context = &self.context.tx_context;
        // The transaction version, ignoring the only_query bit.
        let version = tx_context.tx_info.version();
        let versioned_constants = &tx_context.block_context.versioned_constants;
        // The set of v1-bound-accounts.
        let v1_bound_accounts = &versioned_constants.os_constants.v1_bound_accounts_cairo0;

        // If the transaction version is 3 and the account is in the v1-bound-accounts set,
        // the syscall should return transaction version 1 instead.
        // In such a case, `self.tx_info_start_ptr` is not used.
        if version == TransactionVersion::THREE && v1_bound_accounts.contains(&self.class_hash) {
            let tip = match &tx_context.tx_info {
                TransactionInfo::Current(transaction_info) => transaction_info.tip,
                TransactionInfo::Deprecated(_) => {
                    panic!("Transaction info variant doesn't match transaction version")
                }
            };
            if tip <= versioned_constants.os_constants.v1_bound_accounts_max_tip {
                let modified_version = signed_tx_version(
                    &TransactionVersion::ONE,
                    &TransactionOptions { only_query: tx_context.tx_info.only_query() },
                );
                return self.allocate_tx_info_segment(vm, Some(modified_version));
            }
        }

        match self.tx_info_start_ptr {
            Some(tx_info_start_ptr) => Ok(tx_info_start_ptr),
            None => {
                let tx_info_start_ptr = self.allocate_tx_info_segment(vm, None)?;
                self.tx_info_start_ptr = Some(tx_info_start_ptr);
                Ok(tx_info_start_ptr)
            }
        }
    }

    fn allocate_tx_signature_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> DeprecatedSyscallResult<Relocatable> {
        let signature = &self.context.tx_context.tx_info.signature().0;
        let signature: Vec<MaybeRelocatable> =
            signature.iter().map(|&x| MaybeRelocatable::from(x)).collect();
        let signature_segment_start_ptr = self.read_only_segments.allocate(vm, &signature)?;

        Ok(signature_segment_start_ptr)
    }

    /// Allocates and populates a segment with the transaction info.
    ///
    /// If `tx_version_override` is given, it will be used instead of the real value.
    fn allocate_tx_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
        tx_version_override: Option<TransactionVersion>,
    ) -> DeprecatedSyscallResult<Relocatable> {
        let tx_signature_start_ptr = self.get_or_allocate_tx_signature_segment(vm)?;
        let TransactionContext { block_context, tx_info } = self.context.tx_context.as_ref();
        let tx_signature_length = tx_info.signature().0.len();
        let tx_version = tx_version_override.unwrap_or(tx_info.signed_version());
        let tx_info: Vec<MaybeRelocatable> = vec![
            tx_version.0.into(),
            (*tx_info.sender_address().0.key()).into(),
            Felt::from(tx_info.max_fee_for_execution_info_syscall().0).into(),
            tx_signature_length.into(),
            tx_signature_start_ptr.into(),
            tx_info.transaction_hash().0.into(),
            Felt::from_hex(block_context.chain_info.chain_id.as_hex().as_str())?.into(),
            tx_info.nonce().0.into(),
        ];

        let tx_info_start_ptr = self.read_only_segments.allocate(vm, &tx_info)?;
        Ok(tx_info_start_ptr)
    }

    pub fn get_contract_storage_at(
        &mut self,
        key: StorageKey,
    ) -> DeprecatedSyscallResult<StorageReadResponse> {
        self.accessed_keys.insert(key);
        let value = self.state.get_storage_at(self.storage_address, key)?;
        self.read_values.push(value);

        Ok(StorageReadResponse { value })
    }

    pub fn set_contract_storage_at(
        &mut self,
        key: StorageKey,
        value: Felt,
    ) -> DeprecatedSyscallResult<StorageWriteResponse> {
        self.accessed_keys.insert(key);
        self.state.set_storage_at(self.storage_address, key, value)?;

        Ok(StorageWriteResponse {})
    }

    pub fn get_block_info(&self) -> &BlockInfo {
        &self.context.tx_context.block_context.block_info
    }

    fn delegate_call_helper(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        call_to_external: bool,
    ) -> DeprecatedSyscallResult<DelegateCallResponse> {
        let storage_address = request.contract_address;
        let class_hash = syscall_handler.state.get_class_hash_at(storage_address)?;
        let retdata_segment = execute_library_call(
            syscall_handler,
            vm,
            class_hash,
            Some(storage_address),
            call_to_external,
            request.function_selector,
            request.calldata,
        )?;

        Ok(DelegateCallResponse { segment: retdata_segment })
    }
}

impl ResourceTracker for DeprecatedSyscallHintProcessor<'_> {
    fn consumed(&self) -> bool {
        self.context.vm_run_resources.consumed()
    }

    fn consume_step(&mut self) {
        self.context.vm_run_resources.consume_step()
    }

    fn get_n_steps(&self) -> Option<usize> {
        self.context.vm_run_resources.get_n_steps()
    }

    fn run_resources(&self) -> &RunResources {
        self.context.vm_run_resources.run_resources()
    }
}

impl HintProcessorLogic for DeprecatedSyscallHintProcessor<'_> {
    fn execute_hint(
        &mut self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        hint_data: &Box<dyn Any>,
        constants: &HashMap<String, Felt>,
    ) -> HintExecutionResult {
        let hint = hint_data.downcast_ref::<HintProcessorData>().ok_or(HintError::WrongHintData)?;
        if hint_code::SYSCALL_HINTS.contains(hint.code.as_str()) {
            return Ok(execute_next_deprecated_syscall(
                self,
                vm,
                &hint.ids_data,
                &hint.ap_tracking,
            )?);
        }

        self.builtin_hint_processor.execute_hint(vm, exec_scopes, hint_data, constants)
    }
}

impl DeprecatedSyscallExecutor for DeprecatedSyscallHintProcessor<'_> {
    type Error = DeprecatedSyscallExecutionError;

    fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> DeprecatedSyscallResult<()> {
        if actual_ptr != self.syscall_ptr {
            return Err(DeprecatedSyscallExecutorBaseError::BadSyscallPointer {
                expected_ptr: self.syscall_ptr,
                actual_ptr,
            })?;
        }

        Ok(())
    }

    fn increment_syscall_count(&mut self, selector: &DeprecatedSyscallSelector) {
        self.syscalls_usage.entry(*selector).or_default().increment_call_count();
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        &mut self.syscall_ptr
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<CallContractResponse> {
        let storage_address = request.contract_address;
        let class_hash = syscall_handler.state.get_class_hash_at(storage_address)?;
        let selector = request.function_selector;
        // Check that the call is legal if in Validate execution mode.
        if syscall_handler.is_validate_mode() && syscall_handler.storage_address != storage_address
        {
            return Err(DeprecatedSyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "call_contract".to_string(),
                execution_mode: syscall_handler.execution_mode(),
            });
        }
        let versioned_constants =
            &syscall_handler.context.tx_context.block_context.versioned_constants;
        if versioned_constants.block_direct_execute_call
            && selector == selector_from_name(EXECUTE_ENTRY_POINT_NAME)
        {
            return Err(DeprecatedSyscallExecutionError::DirectExecuteCall);
        }
        let entry_point = CallEntryPoint {
            class_hash: None,
            code_address: Some(storage_address),
            entry_point_type: EntryPointType::External,
            entry_point_selector: selector,
            calldata: request.calldata,
            storage_address,
            caller_address: syscall_handler.storage_address,
            call_type: CallType::Call,
            initial_gas: syscall_handler.context.gas_costs().base.default_initial_gas_cost,
        };
        let retdata_segment =
            execute_inner_call(entry_point, vm, syscall_handler).map_err(|error| {
                error.as_call_contract_execution_error(class_hash, storage_address, selector)
            })?;

        Ok(CallContractResponse { segment: retdata_segment })
    }

    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse> {
        Self::delegate_call_helper(request, vm, syscall_handler, true)
    }

    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse> {
        Self::delegate_call_helper(request, vm, syscall_handler, false)
    }

    fn deploy(
        request: DeployRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DeployResponse> {
        let versioned_constants =
            &syscall_handler.context.tx_context.block_context.versioned_constants;
        if should_reject_deploy(
            versioned_constants.disable_deploy_in_validation_mode,
            syscall_handler.execution_mode(),
        ) {
            return Err(DeprecatedSyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "deploy".to_string(),
                execution_mode: syscall_handler.execution_mode(),
            });
        }

        let deployer_address = syscall_handler.storage_address;
        let deployer_address_for_calculation = match request.deploy_from_zero {
            true => ContractAddress::default(),
            false => deployer_address,
        };
        let deployed_contract_address = calculate_contract_address(
            request.contract_address_salt,
            request.class_hash,
            &request.constructor_calldata,
            deployer_address_for_calculation,
        )?;

        // Increment the Deploy syscall's linear cost counter by the number of elements in the
        // constructor calldata.
        let syscall_usage = syscall_handler
            .syscalls_usage
            .get_mut(&DeprecatedSyscallSelector::Deploy)
            .expect("syscalls_usage entry for Deploy must be initialized");
        syscall_usage.linear_factor += request.constructor_calldata.0.len();

        let ctor_context = ConstructorContext {
            class_hash: request.class_hash,
            code_address: Some(deployed_contract_address),
            storage_address: deployed_contract_address,
            caller_address: deployer_address,
        };
        let mut remaining_gas = syscall_handler.context.gas_costs().base.default_initial_gas_cost;
        let call_info = execute_deployment(
            syscall_handler.state,
            syscall_handler.context,
            ctor_context,
            request.constructor_calldata,
            &mut remaining_gas,
        )?;
        syscall_handler.inner_calls.push(call_info);

        Ok(DeployResponse { contract_address: deployed_contract_address })
    }

    fn emit_event(
        request: EmitEventRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<EmitEventResponse> {
        let execution_context = &mut syscall_handler.context;
        exceeds_event_size_limit(
            execution_context.versioned_constants(),
            execution_context.n_emitted_events + 1,
            &request.content,
        )?;
        let ordered_event =
            OrderedEvent { order: execution_context.n_emitted_events, event: request.content };
        syscall_handler.events.push(ordered_event);
        execution_context.n_emitted_events += 1;

        Ok(EmitEventResponse {})
    }

    fn get_block_number(
        _request: GetBlockNumberRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockNumberResponse> {
        let versioned_constants = syscall_handler.context.versioned_constants();
        let block_number = syscall_handler.get_block_info().block_number;
        let block_number = match syscall_handler.execution_mode() {
            ExecutionMode::Validate => {
                let validate_block_number_rounding =
                    versioned_constants.get_validate_block_number_rounding();
                BlockNumber(
                    (block_number.0 / validate_block_number_rounding)
                        * validate_block_number_rounding,
                )
            }
            ExecutionMode::Execute => block_number,
        };
        Ok(GetBlockNumberResponse { block_number })
    }

    fn get_block_timestamp(
        _request: GetBlockTimestampRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockTimestampResponse> {
        let versioned_constants = syscall_handler.context.versioned_constants();
        let block_timestamp = syscall_handler.get_block_info().block_timestamp;
        let block_timestamp = match syscall_handler.execution_mode() {
            ExecutionMode::Validate => {
                let validate_timestamp_rounding =
                    versioned_constants.get_validate_timestamp_rounding();
                BlockTimestamp(
                    (block_timestamp.0 / validate_timestamp_rounding) * validate_timestamp_rounding,
                )
            }
            ExecutionMode::Execute => block_timestamp,
        };
        Ok(GetBlockTimestampResponse { block_timestamp })
    }

    fn get_caller_address(
        _request: GetCallerAddressRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetCallerAddressResponse> {
        Ok(GetCallerAddressResponse { address: syscall_handler.caller_address })
    }

    fn get_contract_address(
        _request: GetContractAddressRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetContractAddressResponse> {
        Ok(GetContractAddressResponse { address: syscall_handler.storage_address })
    }

    fn get_sequencer_address(
        _request: GetSequencerAddressRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetSequencerAddressResponse> {
        syscall_handler.verify_not_in_validate_mode("get_sequencer_address")?;
        Ok(GetSequencerAddressResponse {
            address: syscall_handler.get_block_info().sequencer_address,
        })
    }

    fn get_tx_info(
        _request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxInfoResponse> {
        let tx_info_start_ptr = syscall_handler.get_or_allocate_tx_info_start_ptr(vm)?;

        Ok(GetTxInfoResponse { tx_info_start_ptr })
    }

    fn get_tx_signature(
        _request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxSignatureResponse> {
        let start_ptr = syscall_handler.get_or_allocate_tx_signature_segment(vm)?;
        let length = syscall_handler.context.tx_context.tx_info.signature().0.len();

        Ok(GetTxSignatureResponse { segment: ReadOnlySegment { start_ptr, length } })
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse> {
        let call_to_external = true;
        let retdata_segment = execute_library_call(
            syscall_handler,
            vm,
            request.class_hash,
            None,
            call_to_external,
            request.function_selector,
            request.calldata,
        )?;

        Ok(LibraryCallResponse { segment: retdata_segment })
    }

    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse> {
        let call_to_external = false;
        let retdata_segment = execute_library_call(
            syscall_handler,
            vm,
            request.class_hash,
            None,
            call_to_external,
            request.function_selector,
            request.calldata,
        )?;

        Ok(LibraryCallResponse { segment: retdata_segment })
    }

    fn replace_class(
        request: ReplaceClassRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<ReplaceClassResponse> {
        // Ensure the class is declared (by reading it).
        syscall_handler.state.get_compiled_class(request.class_hash)?;
        syscall_handler
            .state
            .set_class_hash_at(syscall_handler.storage_address, request.class_hash)?;

        Ok(ReplaceClassResponse {})
    }

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<SendMessageToL1Response> {
        let execution_context = &mut syscall_handler.context;
        if !execution_context.tx_context.block_context.chain_info.is_l3 {
            EthAddress::try_from(request.message.to_address)?;
        }
        let ordered_message_to_l1 = OrderedL2ToL1Message {
            order: execution_context.n_sent_messages_to_l1,
            message: request.message,
        };
        syscall_handler.l2_to_l1_messages.push(ordered_message_to_l1);
        execution_context.n_sent_messages_to_l1 += 1;

        Ok(SendMessageToL1Response {})
    }

    fn storage_read(
        request: StorageReadRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageReadResponse> {
        syscall_handler.get_contract_storage_at(request.address)
    }

    fn storage_write(
        request: StorageWriteRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageWriteResponse> {
        syscall_handler.set_contract_storage_at(request.address, request.value)
    }
}

pub fn felt_to_bool(felt: Felt) -> DeprecatedSyscallExecutorBaseResult<bool> {
    if felt == Felt::ZERO {
        Ok(false)
    } else if felt == Felt::ONE {
        Ok(true)
    } else {
        Err(DeprecatedSyscallExecutorBaseError::InvalidSyscallInput {
            input: felt,
            info: String::from(
                "The deploy_from_zero field in the deploy system call must be 0 or 1.",
            ),
        })
    }
}

pub fn read_calldata(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> DeprecatedSyscallExecutorBaseResult<Calldata> {
    Ok(Calldata(read_felt_array::<DeprecatedSyscallExecutorBaseError>(vm, ptr)?.into()))
}

pub fn read_call_params(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> DeprecatedSyscallExecutorBaseResult<(EntryPointSelector, Calldata)> {
    let function_selector = EntryPointSelector(felt_from_ptr(vm, ptr)?);
    let calldata = read_calldata(vm, ptr)?;

    Ok((function_selector, calldata))
}

pub fn execute_inner_call(
    call: CallEntryPoint,
    vm: &mut VirtualMachine,
    syscall_handler: &mut DeprecatedSyscallHintProcessor<'_>,
) -> DeprecatedSyscallResult<ReadOnlySegment> {
    let mut remaining_gas = call.initial_gas;
    // Use `non_reverting_execute` since we don't support reverts here.
    let call_info = call.non_reverting_execute(
        syscall_handler.state,
        syscall_handler.context,
        &mut remaining_gas,
    )?;
    let retdata = &call_info.execution.retdata.0;
    let retdata: Vec<MaybeRelocatable> =
        retdata.iter().map(|&x| MaybeRelocatable::from(x)).collect();
    let retdata_segment_start_ptr = syscall_handler.read_only_segments.allocate(vm, &retdata)?;

    syscall_handler.inner_calls.push(call_info);
    Ok(ReadOnlySegment { start_ptr: retdata_segment_start_ptr, length: retdata.len() })
}

pub fn execute_library_call(
    syscall_handler: &mut DeprecatedSyscallHintProcessor<'_>,
    vm: &mut VirtualMachine,
    class_hash: ClassHash,
    code_address: Option<ContractAddress>,
    call_to_external: bool,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
) -> DeprecatedSyscallResult<ReadOnlySegment> {
    let entry_point_type =
        if call_to_external { EntryPointType::External } else { EntryPointType::L1Handler };
    let entry_point = CallEntryPoint {
        class_hash: Some(class_hash),
        code_address,
        entry_point_type,
        entry_point_selector,
        calldata,
        // The call context remains the same in a library call.
        storage_address: syscall_handler.storage_address,
        caller_address: syscall_handler.caller_address,
        call_type: CallType::Delegate,
        initial_gas: syscall_handler.context.gas_costs().base.default_initial_gas_cost,
    };

    execute_inner_call(entry_point, vm, syscall_handler).map_err(|error| {
        error.as_lib_call_execution_error(
            class_hash,
            syscall_handler.storage_address,
            entry_point_selector,
        )
    })
}

pub fn read_felt_array<TErr>(vm: &VirtualMachine, ptr: &mut Relocatable) -> Result<Vec<Felt>, TErr>
where
    TErr: From<StarknetApiError>
        + From<VirtualMachineError>
        + From<MemoryError>
        + From<MathError>
        + From<TryFromBigIntError<BigUint>>,
{
    let array_size = felt_from_ptr(vm, ptr)?;
    let array_data_start_ptr = vm.get_relocatable(*ptr)?;
    *ptr = (*ptr + 1)?;

    Ok(felt_range_from_ptr(vm, array_data_start_ptr, usize::try_from(array_size.to_biguint())?)?)
}
