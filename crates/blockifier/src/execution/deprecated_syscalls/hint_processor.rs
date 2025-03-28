use std::any::Any;
use std::collections::{HashMap, HashSet};

use cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::{
    BuiltinHintProcessor,
    HintProcessorData,
};
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_ptr_from_var_name;
use cairo_vm::hint_processor::hint_processor_definition::{HintProcessorLogic, HintReference};
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::runners::cairo_runner::{ResourceTracker, RunResources};
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::{BigUint, TryFromBigIntError};
use starknet_api::block::BlockInfo;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::state::StorageKey;
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
use crate::execution::deprecated_syscalls::{
    call_contract,
    delegate_call,
    delegate_l1_handler,
    deploy,
    emit_event,
    get_block_number,
    get_block_timestamp,
    get_caller_address,
    get_contract_address,
    get_sequencer_address,
    get_tx_info,
    get_tx_signature,
    library_call,
    library_call_l1_handler,
    replace_class,
    send_message_to_l1,
    storage_read,
    storage_write,
    DeprecatedSyscallResult,
    DeprecatedSyscallSelector,
    StorageReadResponse,
    StorageWriteResponse,
    SyscallRequest,
    SyscallResponse,
};
use crate::execution::entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::execution_utils::{
    felt_from_ptr,
    felt_range_from_ptr,
    ReadOnlySegment,
    ReadOnlySegments,
};
use crate::execution::hint_code;
use crate::execution::syscalls::hint_processor::{EmitEventError, SyscallUsageMap};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::transaction::objects::TransactionInfo;

#[derive(Debug, Error)]
pub enum DeprecatedSyscallExecutionError {
    #[error("Bad syscall_ptr; expected: {expected_ptr:?}, got: {actual_ptr:?}.")]
    BadSyscallPointer { expected_ptr: Relocatable, actual_ptr: Relocatable },
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
    #[error("Invalid syscall selector: {0:?}.")]
    InvalidDeprecatedSyscallSelector(Felt),
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

    pub fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> DeprecatedSyscallResult<()> {
        if actual_ptr != self.syscall_ptr {
            return Err(DeprecatedSyscallExecutionError::BadSyscallPointer {
                expected_ptr: self.syscall_ptr,
                actual_ptr,
            });
        }

        Ok(())
    }

    /// Infers and executes the next syscall.
    /// Must comply with the API of a hint function, as defined by the `HintProcessor`.
    pub fn execute_next_syscall(
        &mut self,
        vm: &mut VirtualMachine,
        ids_data: &HashMap<String, HintReference>,
        ap_tracking: &ApTracking,
    ) -> HintExecutionResult {
        let initial_syscall_ptr = get_ptr_from_var_name("syscall_ptr", vm, ids_data, ap_tracking)?;
        self.verify_syscall_ptr(initial_syscall_ptr)?;

        let selector = DeprecatedSyscallSelector::try_from(self.read_next_syscall_selector(vm)?)?;
        self.increment_syscall_count(&selector);

        match selector {
            DeprecatedSyscallSelector::CallContract => self.execute_syscall(vm, call_contract),
            DeprecatedSyscallSelector::DelegateCall => self.execute_syscall(vm, delegate_call),
            DeprecatedSyscallSelector::DelegateL1Handler => {
                self.execute_syscall(vm, delegate_l1_handler)
            }
            DeprecatedSyscallSelector::Deploy => self.execute_syscall(vm, deploy),
            DeprecatedSyscallSelector::EmitEvent => self.execute_syscall(vm, emit_event),
            DeprecatedSyscallSelector::GetBlockNumber => self.execute_syscall(vm, get_block_number),
            DeprecatedSyscallSelector::GetBlockTimestamp => {
                self.execute_syscall(vm, get_block_timestamp)
            }
            DeprecatedSyscallSelector::GetCallerAddress => {
                self.execute_syscall(vm, get_caller_address)
            }
            DeprecatedSyscallSelector::GetContractAddress => {
                self.execute_syscall(vm, get_contract_address)
            }
            DeprecatedSyscallSelector::GetSequencerAddress => {
                self.execute_syscall(vm, get_sequencer_address)
            }
            DeprecatedSyscallSelector::GetTxInfo => self.execute_syscall(vm, get_tx_info),
            DeprecatedSyscallSelector::GetTxSignature => self.execute_syscall(vm, get_tx_signature),
            DeprecatedSyscallSelector::LibraryCall => self.execute_syscall(vm, library_call),
            DeprecatedSyscallSelector::LibraryCallL1Handler => {
                self.execute_syscall(vm, library_call_l1_handler)
            }
            DeprecatedSyscallSelector::ReplaceClass => self.execute_syscall(vm, replace_class),
            DeprecatedSyscallSelector::SendMessageToL1 => {
                self.execute_syscall(vm, send_message_to_l1)
            }
            DeprecatedSyscallSelector::StorageRead => self.execute_syscall(vm, storage_read),
            DeprecatedSyscallSelector::StorageWrite => self.execute_syscall(vm, storage_write),
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            )),
        }
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

    fn execute_syscall<Request, Response, ExecuteCallback>(
        &mut self,
        vm: &mut VirtualMachine,
        execute_callback: ExecuteCallback,
    ) -> HintExecutionResult
    where
        Request: SyscallRequest,
        Response: SyscallResponse,
        ExecuteCallback: FnOnce(
            Request,
            &mut VirtualMachine,
            &mut DeprecatedSyscallHintProcessor<'_>,
        ) -> DeprecatedSyscallResult<Response>,
    {
        let request = Request::read(vm, &mut self.syscall_ptr)?;

        let response = execute_callback(request, vm, self)?;
        response.write(vm, &mut self.syscall_ptr)?;

        Ok(())
    }

    fn read_next_syscall_selector(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> DeprecatedSyscallResult<Felt> {
        Ok(felt_from_ptr(vm, &mut self.syscall_ptr)?)
    }

    fn increment_syscall_count(&mut self, selector: &DeprecatedSyscallSelector) {
        self.syscalls_usage.entry(*selector).or_default().increment_call_count();
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
            return self.execute_next_syscall(vm, &hint.ids_data, &hint.ap_tracking);
        }

        self.builtin_hint_processor.execute_hint(vm, exec_scopes, hint_data, constants)
    }
}

pub fn felt_to_bool(felt: Felt) -> DeprecatedSyscallResult<bool> {
    if felt == Felt::ZERO {
        Ok(false)
    } else if felt == Felt::ONE {
        Ok(true)
    } else {
        Err(DeprecatedSyscallExecutionError::InvalidSyscallInput {
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
) -> DeprecatedSyscallResult<Calldata> {
    Ok(Calldata(read_felt_array::<DeprecatedSyscallExecutionError>(vm, ptr)?.into()))
}

pub fn read_call_params(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> DeprecatedSyscallResult<(EntryPointSelector, Calldata)> {
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
