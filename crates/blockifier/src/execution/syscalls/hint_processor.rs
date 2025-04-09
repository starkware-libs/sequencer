use std::any::Any;
use std::collections::HashMap;

use cairo_lang_casm::hints::{Hint, StarknetHint};
use cairo_lang_runner::casm_run::execute_core_hint_base;
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
use num_traits::ToPrimitive;
use starknet_api::block::BlockHash;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    valid_resource_bounds_as_felts,
    Calldata,
    ResourceAsFelts,
};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::{Felt, FromStrError};
use thiserror::Error;

use crate::abi::sierra_types::SierraTypeError;
use crate::blockifier_versioned_constants::{GasCosts, GasCostsError, SyscallGasCost};
use crate::execution::common_hints::{ExecutionMode, HintExecutionResult};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    EntryPointExecutionContext,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::execution_utils::{
    felt_from_ptr,
    felt_range_from_ptr,
    write_maybe_relocatable,
    ReadOnlySegment,
    ReadOnlySegments,
};
use crate::execution::syscalls::secp::{
    Secp256r1NewRequest,
    Secp256r1NewResponse,
    SecpAddRequest,
    SecpAddResponse,
    SecpGetPointFromXRequest,
    SecpGetPointFromXResponse,
    SecpGetXyRequest,
    SecpGetXyResponse,
    SecpHintProcessor,
    SecpMulRequest,
    SecpMulResponse,
    SecpNewRequest,
    SecpNewResponse,
};
use crate::execution::syscalls::syscall_base::SyscallHandlerBase;
use crate::execution::syscalls::syscall_executor::SyscallExecutor;
use crate::execution::syscalls::{
    CallContractRequest,
    CallContractResponse,
    DeployRequest,
    DeployResponse,
    EmitEventRequest,
    EmitEventResponse,
    GetBlockHashRequest,
    GetBlockHashResponse,
    GetClassHashAtRequest,
    GetClassHashAtResponse,
    GetExecutionInfoRequest,
    GetExecutionInfoResponse,
    KeccakRequest,
    KeccakResponse,
    LibraryCallRequest,
    LibraryCallResponse,
    MetaTxV0Request,
    MetaTxV0Response,
    ReplaceClassRequest,
    ReplaceClassResponse,
    SendMessageToL1Request,
    SendMessageToL1Response,
    Sha256ProcessBlockRequest,
    Sha256ProcessBlockResponse,
    StorageReadRequest,
    StorageReadResponse,
    StorageWriteRequest,
    StorageWriteResponse,
    SyscallRequest,
    SyscallResult,
    SyscallSelector,
};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::transaction::objects::{CurrentTransactionInfo, TransactionInfo};

#[derive(Clone, Debug, Default)]
pub struct SyscallUsage {
    pub call_count: usize,
    pub linear_factor: usize,
}

impl SyscallUsage {
    pub fn new(call_count: usize, linear_factor: usize) -> Self {
        SyscallUsage { call_count, linear_factor }
    }

    pub fn increment_call_count(&mut self) {
        self.call_count += 1;
    }
}

pub type SyscallUsageMap = HashMap<SyscallSelector, SyscallUsage>;

#[derive(Debug, Error)]
pub enum SyscallExecutionError {
    #[error("Bad syscall_ptr; expected: {expected_ptr:?}, got: {actual_ptr:?}.")]
    BadSyscallPointer { expected_ptr: Relocatable, actual_ptr: Relocatable },
    #[error(transparent)]
    EmitEventError(#[from] EmitEventError),
    #[error("Cannot replace V1 class hash with V0 class hash: {class_hash}.")]
    ForbiddenClassReplacement { class_hash: ClassHash },
    #[error(transparent)]
    FromStr(#[from] FromStrError),
    #[error("Invalid address domain: {address_domain}.")]
    InvalidAddressDomain { address_domain: Felt },
    #[error(transparent)]
    ConstructorEntryPointExecutionError(#[from] ConstructorEntryPointExecutionError),
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error("{error}")]
    CallContractExecutionError {
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
        error: Box<SyscallExecutionError>,
    },
    #[error("{error}")]
    LibraryCallExecutionError {
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
        error: Box<SyscallExecutionError>,
    },
    #[error("Invalid syscall input: {input:?}; {info}")]
    InvalidSyscallInput { input: Felt, info: String },
    #[error("Invalid syscall selector: {0:?}.")]
    InvalidSyscallSelector(Felt),
    #[error("Unauthorized syscall {syscall_name} in execution mode {execution_mode}.")]
    InvalidSyscallInExecutionMode { syscall_name: String, execution_mode: ExecutionMode },
    #[error(transparent)]
    MathError(#[from] cairo_vm::types::errors::math_errors::MathError),
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error(transparent)]
    SierraTypeError(#[from] SierraTypeError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    VirtualMachineError(#[from] VirtualMachineError),
    #[error("Syscall revert.")]
    Revert { error_data: Vec<Felt> },
}

#[derive(Debug, Error)]
pub enum EmitEventError {
    #[error(
        "Exceeded the maximum keys length, keys length: {keys_length}, max keys length: \
         {max_keys_length}."
    )]
    ExceedsMaxKeysLength { keys_length: usize, max_keys_length: usize },
    #[error(
        "Exceeded the maximum data length, data length: {data_length}, max data length: \
         {max_data_length}."
    )]
    ExceedsMaxDataLength { data_length: usize, max_data_length: usize },
    #[error(
        "Exceeded the maximum number of events, number events: {n_emitted_events}, max number \
         events: {max_n_emitted_events}."
    )]
    ExceedsMaxNumberOfEmittedEvents { n_emitted_events: usize, max_n_emitted_events: usize },
}

// Needed for custom hint implementations (in our case, syscall hints) which must comply with the
// cairo-rs API.
impl From<SyscallExecutionError> for HintError {
    fn from(error: SyscallExecutionError) -> Self {
        HintError::Internal(VirtualMachineError::Other(error.into()))
    }
}

impl SyscallExecutionError {
    pub fn as_call_contract_execution_error(
        self,
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
    ) -> Self {
        SyscallExecutionError::CallContractExecutionError {
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
        SyscallExecutionError::LibraryCallExecutionError {
            class_hash,
            storage_address,
            selector,
            error: Box::new(self),
        }
    }
}

/// Error codes returned by Cairo 1.0 code.
// "Out of gas";
pub const OUT_OF_GAS_ERROR: &str =
    "0x000000000000000000000000000000000000000000004f7574206f6620676173";
// "Block number out of range";
pub const BLOCK_NUMBER_OUT_OF_RANGE_ERROR: &str =
    "0x00000000000000426c6f636b206e756d626572206f7574206f662072616e6765";
// "ENTRYPOINT_NOT_FOUND";
pub const ENTRYPOINT_NOT_FOUND_ERROR: &str =
    "0x000000000000000000000000454e545259504f494e545f4e4f545f464f554e44";
// "ENTRYPOINT_FAILED";
pub const ENTRYPOINT_FAILED_ERROR: &str =
    "0x000000000000000000000000000000454e545259504f494e545f4641494c4544";
// "Invalid input length";
pub const INVALID_INPUT_LENGTH_ERROR: &str =
    "0x000000000000000000000000496e76616c696420696e707574206c656e677468";
// "Invalid argument";
pub const INVALID_ARGUMENT: &str =
    "0x00000000000000000000000000000000496e76616c696420617267756d656e74";

/// Executes Starknet syscalls (stateful protocol hints) during the execution of an entry point
/// call.
pub struct SyscallHintProcessor<'a> {
    pub base: Box<SyscallHandlerBase<'a>>,

    // VM-specific fields.
    pub syscalls_usage: SyscallUsageMap,

    // Fields needed for execution and validation.
    pub read_only_segments: ReadOnlySegments,
    pub syscall_ptr: Relocatable,

    // Secp hint processors.
    pub secp256k1_hint_processor: SecpHintProcessor<ark_secp256k1::Config>,
    pub secp256r1_hint_processor: SecpHintProcessor<ark_secp256r1::Config>,

    pub sha256_segment_end_ptr: Option<Relocatable>,

    // Execution info, for get_execution_info syscall; allocated on-demand.
    execution_info_ptr: Option<Relocatable>,

    // Additional fields.
    hints: &'a HashMap<String, Hint>,
}

impl<'a> SyscallHintProcessor<'a> {
    pub fn new(
        state: &'a mut dyn State,
        context: &'a mut EntryPointExecutionContext,
        initial_syscall_ptr: Relocatable,
        call: ExecutableCallEntryPoint,
        hints: &'a HashMap<String, Hint>,
        read_only_segments: ReadOnlySegments,
    ) -> Self {
        SyscallHintProcessor {
            base: Box::new(SyscallHandlerBase::new(call, state, context)),
            syscalls_usage: SyscallUsageMap::default(),
            read_only_segments,
            syscall_ptr: initial_syscall_ptr,
            hints,
            execution_info_ptr: None,
            secp256k1_hint_processor: SecpHintProcessor::default(),
            secp256r1_hint_processor: SecpHintProcessor::default(),
            sha256_segment_end_ptr: None,
        }
    }

    pub fn storage_address(&self) -> ContractAddress {
        self.base.call.storage_address
    }

    pub fn caller_address(&self) -> ContractAddress {
        self.base.call.caller_address
    }

    pub fn entry_point_selector(&self) -> EntryPointSelector {
        self.base.call.entry_point_selector
    }

    pub fn execution_mode(&self) -> ExecutionMode {
        self.base.context.execution_mode
    }

    pub fn is_validate_mode(&self) -> bool {
        self.execution_mode() == ExecutionMode::Validate
    }

    pub fn gas_costs(&self) -> &GasCosts {
        self.base.context.gas_costs()
    }

    /// Infers and executes the next syscall.
    /// Must comply with the API of a hint function, as defined by the `HintProcessor`.
    pub fn execute_next_syscall(
        &mut self,
        vm: &mut VirtualMachine,
        hint: &StarknetHint,
    ) -> HintExecutionResult {
        let StarknetHint::SystemCall { .. } = hint else {
            return Err(HintError::Internal(VirtualMachineError::Other(anyhow::anyhow!(
                "Test functions are unsupported on starknet."
            ))));
        };

        let selector = SyscallSelector::try_from(self.read_next_syscall_selector(vm)?)?;

        // Keccak resource usage depends on the input length, so we increment the syscall count
        // in the syscall execution callback.
        if selector != SyscallSelector::Keccak {
            self.increment_syscall_count(&selector);
        }

        self.execute_syscall_from_selector(vm, selector)
    }

    pub fn get_or_allocate_execution_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> SyscallResult<Relocatable> {
        // Note: the returned version in the transaction info struct might not be equal to the
        // actual transaction version.
        // Also, the returned version is a property of the current entry-point execution,
        // so it is okay to allocate and cache it once without re-checking the version in every
        // `get_execution_info` syscall invocation.
        match self.execution_info_ptr {
            Some(execution_info_ptr) => Ok(execution_info_ptr),
            None => {
                let execution_info_ptr = self.allocate_execution_info_segment(vm)?;
                self.execution_info_ptr = Some(execution_info_ptr);
                Ok(execution_info_ptr)
            }
        }
    }

    fn allocate_tx_resource_bounds_segment(
        &mut self,
        vm: &mut VirtualMachine,
        tx_info: &CurrentTransactionInfo,
        exclude_l1_data_gas: bool,
    ) -> SyscallResult<(Relocatable, Relocatable)> {
        let flat_resource_bounds: Vec<_> =
            valid_resource_bounds_as_felts(&tx_info.resource_bounds, exclude_l1_data_gas)?
                .into_iter()
                .flat_map(ResourceAsFelts::flatten)
                .collect();

        self.allocate_data_segment(vm, &flat_resource_bounds)
    }

    fn read_next_syscall_selector(&mut self, vm: &mut VirtualMachine) -> SyscallResult<Felt> {
        Ok(felt_from_ptr(vm, &mut self.syscall_ptr)?)
    }

    pub fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, n: usize) {
        let syscall_usage = self.syscalls_usage.entry(*selector).or_default();
        syscall_usage.call_count += n;
    }

    fn increment_syscall_count(&mut self, selector: &SyscallSelector) {
        self.increment_syscall_count_by(selector, 1);
    }

    pub fn increment_linear_factor_by(&mut self, selector: &SyscallSelector, n: usize) {
        let syscall_usage = self
            .syscalls_usage
            .get_mut(selector)
            .expect("syscalls_usage entry must be initialized before incrementing linear factor");
        syscall_usage.linear_factor += n;
    }

    fn allocate_execution_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> SyscallResult<Relocatable> {
        let block_info_ptr = self.allocate_block_info_segment(vm)?;
        let tx_info_ptr = self.allocate_tx_info_segment(vm)?;

        let additional_info: Vec<MaybeRelocatable> = vec![
            block_info_ptr.into(),
            tx_info_ptr.into(),
            self.caller_address().0.key().into(),
            self.storage_address().0.key().into(),
            self.entry_point_selector().0.into(),
        ];
        let execution_info_segment_start_ptr =
            self.read_only_segments.allocate(vm, &additional_info)?;

        Ok(execution_info_segment_start_ptr)
    }

    fn allocate_block_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> SyscallResult<Relocatable> {
        let block_info = match self.base.context.execution_mode {
            ExecutionMode::Execute => self.base.context.tx_context.block_context.block_info(),
            ExecutionMode::Validate => {
                &self.base.context.tx_context.block_context.block_info_for_validate()
            }
        };
        let block_data = vec![
            Felt::from(block_info.block_number.0),
            Felt::from(block_info.block_timestamp.0),
            Felt::from(block_info.sequencer_address),
        ];
        let (block_info_segment_start_ptr, _) = self.allocate_data_segment(vm, &block_data)?;

        Ok(block_info_segment_start_ptr)
    }

    fn allocate_data_segment(
        &mut self,
        vm: &mut VirtualMachine,
        data: &[Felt],
    ) -> SyscallResult<(Relocatable, Relocatable)> {
        let data: Vec<MaybeRelocatable> = data.iter().map(|&x| MaybeRelocatable::from(x)).collect();
        let data_segment_start_ptr = self.read_only_segments.allocate(vm, &data)?;
        let data_segment_end_ptr = (data_segment_start_ptr + data.len())?;
        Ok((data_segment_start_ptr, data_segment_end_ptr))
    }

    fn allocate_tx_info_segment(&mut self, vm: &mut VirtualMachine) -> SyscallResult<Relocatable> {
        let tx_info = &self.base.context.tx_context.clone().tx_info;
        let (tx_signature_start_ptr, tx_signature_end_ptr) =
            &self.allocate_data_segment(vm, &tx_info.signature().0)?;

        // Note: the returned version might not be equal to the actual transaction version.
        let returned_version = self.base.tx_version_for_get_execution_info();
        let mut tx_data: Vec<MaybeRelocatable> = vec![
            returned_version.0.into(),
            tx_info.sender_address().0.key().into(),
            Felt::from(tx_info.max_fee_for_execution_info_syscall().0).into(),
            tx_signature_start_ptr.into(),
            tx_signature_end_ptr.into(),
            (tx_info).transaction_hash().0.into(),
            Felt::from_hex(
                self.base.context.tx_context.block_context.chain_info.chain_id.as_hex().as_str(),
            )?
            .into(),
            (tx_info).nonce().0.into(),
        ];

        match tx_info {
            TransactionInfo::Current(context) => {
                let should_exclude_l1_data_gas = self.base.should_exclude_l1_data_gas();
                let (tx_resource_bounds_start_ptr, tx_resource_bounds_end_ptr) = &self
                    .allocate_tx_resource_bounds_segment(vm, context, should_exclude_l1_data_gas)?;

                let (tx_paymaster_data_start_ptr, tx_paymaster_data_end_ptr) =
                    &self.allocate_data_segment(vm, &context.paymaster_data.0)?;

                let (tx_account_deployment_data_start_ptr, tx_account_deployment_data_end_ptr) =
                    &self.allocate_data_segment(vm, &context.account_deployment_data.0)?;

                tx_data.extend_from_slice(&[
                    tx_resource_bounds_start_ptr.into(),
                    tx_resource_bounds_end_ptr.into(),
                    Felt::from(context.tip.0).into(),
                    tx_paymaster_data_start_ptr.into(),
                    tx_paymaster_data_end_ptr.into(),
                    Felt::from(context.nonce_data_availability_mode).into(),
                    Felt::from(context.fee_data_availability_mode).into(),
                    tx_account_deployment_data_start_ptr.into(),
                    tx_account_deployment_data_end_ptr.into(),
                ]);
            }
            TransactionInfo::Deprecated(_) => {
                let zero_felt: MaybeRelocatable = Felt::ZERO.into();
                tx_data.extend_from_slice(&[
                    zero_felt.clone(), // Empty segment of resource bounds (start ptr).
                    zero_felt.clone(), // Empty segment of resource bounds (end ptr).
                    zero_felt.clone(), // Tip.
                    zero_felt.clone(), // Empty segment of paymaster data (start ptr).
                    zero_felt.clone(), // Empty segment of paymaster data (end ptr).
                    zero_felt.clone(), // Nonce DA mode.
                    zero_felt.clone(), // Fee DA mode.
                    zero_felt.clone(), // Empty segment of account deployment data (start ptr).
                    zero_felt,         // Empty segment of account deployment data (end ptr).
                ]);
            }
        };

        let tx_info_start_ptr = self.read_only_segments.allocate(vm, &tx_data)?;
        Ok(tx_info_start_ptr)
    }

    pub fn finalize(&mut self) {
        self.base.finalize();
    }
}

impl SyscallExecutor for SyscallHintProcessor<'_> {
    fn get_gas_cost_from_selector(
        &self,
        selector: &SyscallSelector,
    ) -> Result<SyscallGasCost, GasCostsError> {
        self.gas_costs().syscalls.get_syscall_gas_cost(selector)
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        &mut self.syscall_ptr
    }

    fn get_syscall_base_gas_cost(&self) -> u64 {
        self.base.context.gas_costs().base.syscall_base_gas_cost
    }

    fn update_revert_gas_with_next_remaining_gas(&mut self, remaining_gas: GasAmount) {
        self.base.context.update_revert_gas_with_next_remaining_gas(remaining_gas);
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse> {
        let storage_address = request.contract_address;
        let class_hash = syscall_handler.base.state.get_class_hash_at(storage_address)?;
        let selector = request.function_selector;
        if syscall_handler.is_validate_mode()
            && syscall_handler.storage_address() != storage_address
        {
            return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "call_contract".to_string(),
                execution_mode: syscall_handler.execution_mode(),
            });
        }
        let entry_point = CallEntryPoint {
            class_hash: None,
            code_address: Some(storage_address),
            entry_point_type: EntryPointType::External,
            entry_point_selector: selector,
            calldata: request.calldata,
            storage_address,
            caller_address: syscall_handler.storage_address(),
            call_type: CallType::Call,
            // NOTE: this value might be overridden later on.
            initial_gas: *remaining_gas,
        };

        let retdata_segment = execute_inner_call(entry_point, vm, syscall_handler, remaining_gas)
            .map_err(|error| match error {
            SyscallExecutionError::Revert { .. } => error,
            _ => error.as_call_contract_execution_error(class_hash, storage_address, selector),
        })?;

        Ok(CallContractResponse { segment: retdata_segment })
    }

    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse> {
        // Increment the Deploy syscall's linear cost counter by the number of elements in the
        // constructor calldata.
        syscall_handler.increment_linear_factor_by(
            &SyscallSelector::Deploy,
            request.constructor_calldata.0.len(),
        );

        let (deployed_contract_address, call_info) = syscall_handler.base.deploy(
            request.class_hash,
            request.contract_address_salt,
            request.constructor_calldata,
            request.deploy_from_zero,
            remaining_gas,
        )?;
        let constructor_retdata =
            create_retdata_segment(vm, syscall_handler, &call_info.execution.retdata.0)?;
        syscall_handler.base.inner_calls.push(call_info);

        Ok(DeployResponse { contract_address: deployed_contract_address, constructor_retdata })
    }

    fn emit_event(
        request: EmitEventRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse> {
        syscall_handler.base.emit_event(request.content)?;
        Ok(EmitEventResponse {})
    }

    // TODO(Aner): should this be here or in the trait?
    /// Returns the block hash of a given block_number.
    /// Returns the expected block hash if the given block was created at least
    /// [crate::abi::constants::STORED_BLOCK_HASH_BUFFER] blocks before the current block.
    /// Otherwise, returns an error.
    fn get_block_hash(
        request: GetBlockHashRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse> {
        let block_hash = BlockHash(syscall_handler.base.get_block_hash(request.block_number.0)?);
        Ok(GetBlockHashResponse { block_hash })
    }

    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse> {
        syscall_handler.base.get_class_hash_at(request)
    }

    fn get_execution_info(
        _request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse> {
        let execution_info_ptr = syscall_handler.get_or_allocate_execution_info_segment(vm)?;

        Ok(GetExecutionInfoResponse { execution_info_ptr })
    }

    fn keccak(
        request: KeccakRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<KeccakResponse> {
        let input_length = (request.input_end - request.input_start)?;

        let data = vm.get_integer_range(request.input_start, input_length)?;
        let data_u64: &[u64] = &data
            .iter()
            .map(|felt| {
                felt.to_u64().ok_or_else(|| SyscallExecutionError::InvalidSyscallInput {
                    input: **felt,
                    info: "Invalid input for the keccak syscall.".to_string(),
                })
            })
            .collect::<Result<Vec<u64>, _>>()?;

        let (state, n_rounds) = syscall_handler.base.keccak(data_u64, remaining_gas)?;

        // For the keccak system call we want to count the number of rounds rather than the number
        // of syscall invocations.
        syscall_handler.increment_syscall_count_by(&SyscallSelector::Keccak, n_rounds);

        Ok(KeccakResponse {
            result_low: (Felt::from(state[1]) * Felt::TWO.pow(64_u128)) + Felt::from(state[0]),
            result_high: (Felt::from(state[3]) * Felt::TWO.pow(64_u128)) + Felt::from(state[2]),
        })
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse> {
        let entry_point = CallEntryPoint {
            class_hash: Some(request.class_hash),
            code_address: None,
            entry_point_type: EntryPointType::External,
            entry_point_selector: request.function_selector,
            calldata: request.calldata,
            // The call context remains the same in a library call.
            storage_address: syscall_handler.storage_address(),
            caller_address: syscall_handler.caller_address(),
            call_type: CallType::Delegate,
            // NOTE: this value might be overridden later on.
            initial_gas: *remaining_gas,
        };

        let retdata_segment = execute_inner_call(entry_point, vm, syscall_handler, remaining_gas)
            .map_err(|error| match error {
            SyscallExecutionError::Revert { .. } => error,
            _ => error.as_lib_call_execution_error(
                request.class_hash,
                syscall_handler.storage_address(),
                request.function_selector,
            ),
        })?;

        Ok(LibraryCallResponse { segment: retdata_segment })
    }

    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response> {
        // Increment the MetaTxV0 syscall's linear cost counter by the number of elements in the
        // calldata.
        syscall_handler.increment_linear_factor_by(
            &SyscallSelector::MetaTxV0,
            request.get_linear_factor_length(),
        );

        let storage_address = request.contract_address;
        let selector = request.entry_point_selector;

        let raw_retdata = syscall_handler.base.meta_tx_v0(
            storage_address,
            selector,
            request.calldata,
            request.signature,
            remaining_gas,
        )?;
        let retdata_segment = create_retdata_segment(vm, syscall_handler, &raw_retdata)?;

        Ok(MetaTxV0Response { segment: retdata_segment })
    }

    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse> {
        const SHA256_BLOCK_SIZE: usize = 16;

        let data = vm.get_integer_range(request.input_start, SHA256_BLOCK_SIZE)?;
        const SHA256_STATE_SIZE: usize = 8;
        let prev_state = vm.get_integer_range(request.state_ptr, SHA256_STATE_SIZE)?;

        let data_as_bytes = sha2::digest::generic_array::GenericArray::from_exact_iter(
            data.iter().flat_map(|felt| {
                felt.to_bigint()
                    .to_u32()
                    .expect("libfunc should ensure the input is an [u32; 16].")
                    .to_be_bytes()
            }),
        )
        .expect(
            "u32.to_be_bytes() returns 4 bytes, and data.len() == 16. So data contains 64 bytes.",
        );

        let mut state_as_words: [u32; SHA256_STATE_SIZE] = core::array::from_fn(|i| {
            prev_state[i].to_bigint().to_u32().expect(
                "libfunc only accepts SHA256StateHandle which can only be created from an \
                 Array<u32>.",
            )
        });

        sha2::compress256(&mut state_as_words, &[data_as_bytes]);

        let segment = syscall_handler.sha256_segment_end_ptr.unwrap_or(vm.add_memory_segment());

        let response = segment;
        let data: Vec<MaybeRelocatable> =
            state_as_words.iter().map(|&arg| MaybeRelocatable::from(Felt::from(arg))).collect();

        syscall_handler.sha256_segment_end_ptr = Some(vm.load_data(segment, &data)?);

        Ok(Sha256ProcessBlockResponse { state_ptr: response })
    }

    fn replace_class(
        request: ReplaceClassRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse> {
        syscall_handler.base.replace_class(request.class_hash)?;
        Ok(ReplaceClassResponse {})
    }

    fn secp256k1_add(
        request: SecpAddRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        syscall_handler.secp256k1_hint_processor.secp_add(request)
    }

    fn secp256k1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        syscall_handler.secp256k1_hint_processor.secp_get_point_from_x(request)
    }

    fn secp256k1_get_xy(
        request: SecpGetXyRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        syscall_handler.secp256k1_hint_processor.secp_get_xy(request)
    }

    fn secp256k1_mul(
        request: SecpMulRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        syscall_handler.secp256k1_hint_processor.secp_mul(request)
    }

    fn secp256k1_new(
        request: SecpNewRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse> {
        syscall_handler.secp256k1_hint_processor.secp_new(request)
    }

    fn secp256r1_add(
        request: SecpAddRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        syscall_handler.secp256r1_hint_processor.secp_add(request)
    }

    fn secp256r1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        syscall_handler.secp256r1_hint_processor.secp_get_point_from_x(request)
    }

    fn secp256r1_get_xy(
        request: SecpGetXyRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        syscall_handler.secp256r1_hint_processor.secp_get_xy(request)
    }

    fn secp256r1_mul(
        request: SecpMulRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        syscall_handler.secp256r1_hint_processor.secp_mul(request)
    }

    fn secp256r1_new(
        request: Secp256r1NewRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256r1NewResponse> {
        syscall_handler.secp256r1_hint_processor.secp_new(request)
    }

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response> {
        syscall_handler.base.send_message_to_l1(request.message)?;
        Ok(SendMessageToL1Response {})
    }

    fn storage_read(
        request: StorageReadRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse> {
        let value = syscall_handler.base.storage_read(request.address)?;
        Ok(StorageReadResponse { value })
    }

    fn storage_write(
        request: StorageWriteRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse> {
        syscall_handler.base.storage_write(request.address, request.value)?;
        Ok(StorageWriteResponse {})
    }
}

impl ResourceTracker for SyscallHintProcessor<'_> {
    fn consumed(&self) -> bool {
        self.base.context.vm_run_resources.consumed()
    }

    /// Consumes a single step (if we are in step-tracking mode).
    fn consume_step(&mut self) {
        if *self
            .base
            .context
            .tracked_resource_stack
            .last()
            .expect("When consume_step is called, tracked resource stack is initialized.")
            == TrackedResource::CairoSteps
        {
            self.base.context.vm_run_resources.consume_step();
        }
    }

    fn get_n_steps(&self) -> Option<usize> {
        self.base.context.vm_run_resources.get_n_steps()
    }

    fn run_resources(&self) -> &RunResources {
        self.base.context.vm_run_resources.run_resources()
    }
}

impl HintProcessorLogic for SyscallHintProcessor<'_> {
    fn execute_hint(
        &mut self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        hint_data: &Box<dyn Any>,
        _constants: &HashMap<String, Felt>,
    ) -> HintExecutionResult {
        let hint = hint_data.downcast_ref::<Hint>().ok_or(HintError::WrongHintData)?;
        // Segment arena finalization is relevant only for proof so there is no need to allocate
        // memory segments for it in the sequencer.
        let no_temporary_segments = true;
        match hint {
            Hint::Core(hint) => {
                execute_core_hint_base(vm, exec_scopes, hint, no_temporary_segments)
            }
            Hint::Starknet(hint) => self.execute_next_syscall(vm, hint),
            Hint::External(_) => {
                panic!("starknet should never accept classes with external hints!")
            }
        }
    }

    /// Trait function to store hint in the hint processor by string.
    fn compile_hint(
        &self,
        hint_code: &str,
        _ap_tracking_data: &ApTracking,
        _reference_ids: &HashMap<String, usize>,
        _references: &[HintReference],
    ) -> Result<Box<dyn Any>, VirtualMachineError> {
        Ok(Box::new(self.hints[hint_code].clone()))
    }
}

pub fn felt_to_bool(felt: Felt, error_info: &str) -> SyscallResult<bool> {
    if felt == Felt::ZERO {
        Ok(false)
    } else if felt == Felt::ONE {
        Ok(true)
    } else {
        Err(SyscallExecutionError::InvalidSyscallInput { input: felt, info: error_info.into() })
    }
}

pub fn read_calldata(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<Calldata> {
    Ok(Calldata(read_felt_array::<SyscallExecutionError>(vm, ptr)?.into()))
}

pub fn read_call_params(
    vm: &VirtualMachine,
    ptr: &mut Relocatable,
) -> SyscallResult<(EntryPointSelector, Calldata)> {
    let function_selector = EntryPointSelector(felt_from_ptr(vm, ptr)?);
    let calldata = read_calldata(vm, ptr)?;

    Ok((function_selector, calldata))
}

pub fn execute_inner_call(
    call: CallEntryPoint,
    vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    remaining_gas: &mut u64,
) -> SyscallResult<ReadOnlySegment> {
    let raw_retdata = syscall_handler.base.execute_inner_call(call, remaining_gas)?;
    create_retdata_segment(vm, syscall_handler, &raw_retdata)
}

pub fn create_retdata_segment(
    vm: &mut VirtualMachine,
    syscall_handler: &mut SyscallHintProcessor<'_>,
    raw_retdata: &[Felt],
) -> SyscallResult<ReadOnlySegment> {
    let (retdata_segment_start_ptr, _) = syscall_handler.allocate_data_segment(vm, raw_retdata)?;

    Ok(ReadOnlySegment { start_ptr: retdata_segment_start_ptr, length: raw_retdata.len() })
}

pub fn read_felt_array<TErr>(vm: &VirtualMachine, ptr: &mut Relocatable) -> Result<Vec<Felt>, TErr>
where
    TErr: From<StarknetApiError> + From<VirtualMachineError> + From<MemoryError> + From<MathError>,
{
    let array_data_start_ptr = vm.get_relocatable(*ptr)?;
    *ptr = (*ptr + 1)?;
    let array_data_end_ptr = vm.get_relocatable(*ptr)?;
    *ptr = (*ptr + 1)?;
    let array_size = (array_data_end_ptr - array_data_start_ptr)?;

    Ok(felt_range_from_ptr(vm, array_data_start_ptr, array_size)?)
}

pub fn write_segment(
    vm: &mut VirtualMachine,
    ptr: &mut Relocatable,
    segment: ReadOnlySegment,
) -> SyscallResult<()> {
    write_maybe_relocatable(vm, ptr, segment.start_ptr)?;
    let segment_end_ptr = (segment.start_ptr + segment.length)?;
    write_maybe_relocatable(vm, ptr, segment_end_ptr)?;

    Ok(())
}
