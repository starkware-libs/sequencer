use std::any::Any;
use std::collections::{hash_map, HashMap, HashSet};

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
use cairo_vm::vm::runners::cairo_runner::{ExecutionResources, ResourceTracker, RunResources};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    Resource,
    ValidResourceBounds,
};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::{Felt, FromStrError};
use thiserror::Error;

use crate::abi::sierra_types::SierraTypeError;
use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message};
use crate::execution::common_hints::{ExecutionMode, HintExecutionResult};
use crate::execution::entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::execution_utils::{
    felt_from_ptr,
    felt_range_from_ptr,
    max_fee_for_execution_info,
    write_maybe_relocatable,
    ReadOnlySegment,
    ReadOnlySegments,
};
use crate::execution::syscalls::secp::{
    secp256k1_add,
    secp256k1_get_point_from_x,
    secp256k1_get_xy,
    secp256k1_mul,
    secp256k1_new,
    secp256r1_add,
    secp256r1_get_point_from_x,
    secp256r1_get_xy,
    secp256r1_mul,
    secp256r1_new,
    SecpHintProcessor,
};
use crate::execution::syscalls::{
    call_contract,
    deploy,
    emit_event,
    get_block_hash,
    get_class_hash_at,
    get_execution_info,
    keccak,
    library_call,
    replace_class,
    send_message_to_l1,
    sha_256_process_block,
    storage_read,
    storage_write,
    StorageReadResponse,
    StorageWriteResponse,
    SyscallRequest,
    SyscallRequestWrapper,
    SyscallResponse,
    SyscallResponseWrapper,
    SyscallResult,
    SyscallSelector,
};
use crate::state::errors::StateError;
use crate::state::state_api::State;
use crate::transaction::objects::{CurrentTransactionInfo, TransactionInfo};

pub type SyscallCounter = HashMap<SyscallSelector, usize>;

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
    #[error("Syscall error.")]
    SyscallError { error_data: Vec<Felt> },
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
    // Input for execution.
    pub state: &'a mut dyn State,
    pub resources: &'a mut ExecutionResources,
    pub context: &'a mut EntryPointExecutionContext,
    pub call: CallEntryPoint,

    // Execution results.
    /// Inner calls invoked by the current execution.
    pub inner_calls: Vec<CallInfo>,
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub syscall_counter: SyscallCounter,

    // Fields needed for execution and validation.
    pub read_only_segments: ReadOnlySegments,
    pub syscall_ptr: Relocatable,

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey>,
    pub read_class_hash_values: Vec<ClassHash>,
    // Accessed addresses by the `get_class_hash_at` syscall.
    pub accessed_contract_addresses: HashSet<ContractAddress>,

    // The original storage value of the executed contract.
    // Should be moved back `context.revert_info` before executing an inner call.
    pub original_values: HashMap<StorageKey, Felt>,

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
        resources: &'a mut ExecutionResources,
        context: &'a mut EntryPointExecutionContext,
        initial_syscall_ptr: Relocatable,
        call: CallEntryPoint,
        hints: &'a HashMap<String, Hint>,
        read_only_segments: ReadOnlySegments,
    ) -> Self {
        let original_values = std::mem::take(
            &mut context
                .revert_infos
                .0
                .last_mut()
                .expect("Missing contract revert info.")
                .original_values,
        );
        SyscallHintProcessor {
            state,
            resources,
            context,
            call,
            inner_calls: vec![],
            events: vec![],
            l2_to_l1_messages: vec![],
            syscall_counter: SyscallCounter::default(),
            read_only_segments,
            syscall_ptr: initial_syscall_ptr,
            read_values: vec![],
            accessed_keys: HashSet::new(),
            read_class_hash_values: vec![],
            accessed_contract_addresses: HashSet::new(),
            original_values,
            hints,
            execution_info_ptr: None,
            secp256k1_hint_processor: SecpHintProcessor::default(),
            secp256r1_hint_processor: SecpHintProcessor::default(),
            sha256_segment_end_ptr: None,
        }
    }

    pub fn storage_address(&self) -> ContractAddress {
        self.call.storage_address
    }

    pub fn caller_address(&self) -> ContractAddress {
        self.call.caller_address
    }

    pub fn entry_point_selector(&self) -> EntryPointSelector {
        self.call.entry_point_selector
    }

    pub fn execution_mode(&self) -> ExecutionMode {
        self.context.execution_mode
    }

    pub fn is_validate_mode(&self) -> bool {
        self.execution_mode() == ExecutionMode::Validate
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

        match selector {
            SyscallSelector::CallContract => self.execute_syscall(
                vm,
                call_contract,
                self.context.gas_costs().call_contract_gas_cost,
            ),
            SyscallSelector::GetClassHashAt => self.execute_syscall(
                vm,
                get_class_hash_at,
                self.context.gas_costs().get_class_hash_at_gas_cost,
            ),
            SyscallSelector::Deploy => {
                self.execute_syscall(vm, deploy, self.context.gas_costs().deploy_gas_cost)
            }
            SyscallSelector::EmitEvent => {
                self.execute_syscall(vm, emit_event, self.context.gas_costs().emit_event_gas_cost)
            }
            SyscallSelector::GetBlockHash => self.execute_syscall(
                vm,
                get_block_hash,
                self.context.gas_costs().get_block_hash_gas_cost,
            ),
            SyscallSelector::GetExecutionInfo => self.execute_syscall(
                vm,
                get_execution_info,
                self.context.gas_costs().get_execution_info_gas_cost,
            ),
            SyscallSelector::Keccak => {
                self.execute_syscall(vm, keccak, self.context.gas_costs().keccak_gas_cost)
            }
            SyscallSelector::Sha256ProcessBlock => self.execute_syscall(
                vm,
                sha_256_process_block,
                self.context.gas_costs().sha256_process_block_gas_cost,
            ),
            SyscallSelector::LibraryCall => self.execute_syscall(
                vm,
                library_call,
                self.context.gas_costs().library_call_gas_cost,
            ),
            SyscallSelector::ReplaceClass => self.execute_syscall(
                vm,
                replace_class,
                self.context.gas_costs().replace_class_gas_cost,
            ),
            SyscallSelector::Secp256k1Add => self.execute_syscall(
                vm,
                secp256k1_add,
                self.context.gas_costs().secp256k1_add_gas_cost,
            ),
            SyscallSelector::Secp256k1GetPointFromX => self.execute_syscall(
                vm,
                secp256k1_get_point_from_x,
                self.context.gas_costs().secp256k1_get_point_from_x_gas_cost,
            ),
            SyscallSelector::Secp256k1GetXy => self.execute_syscall(
                vm,
                secp256k1_get_xy,
                self.context.gas_costs().secp256k1_get_xy_gas_cost,
            ),
            SyscallSelector::Secp256k1Mul => self.execute_syscall(
                vm,
                secp256k1_mul,
                self.context.gas_costs().secp256k1_mul_gas_cost,
            ),
            SyscallSelector::Secp256k1New => self.execute_syscall(
                vm,
                secp256k1_new,
                self.context.gas_costs().secp256k1_new_gas_cost,
            ),
            SyscallSelector::Secp256r1Add => self.execute_syscall(
                vm,
                secp256r1_add,
                self.context.gas_costs().secp256r1_add_gas_cost,
            ),
            SyscallSelector::Secp256r1GetPointFromX => self.execute_syscall(
                vm,
                secp256r1_get_point_from_x,
                self.context.gas_costs().secp256r1_get_point_from_x_gas_cost,
            ),
            SyscallSelector::Secp256r1GetXy => self.execute_syscall(
                vm,
                secp256r1_get_xy,
                self.context.gas_costs().secp256r1_get_xy_gas_cost,
            ),
            SyscallSelector::Secp256r1Mul => self.execute_syscall(
                vm,
                secp256r1_mul,
                self.context.gas_costs().secp256r1_mul_gas_cost,
            ),
            SyscallSelector::Secp256r1New => self.execute_syscall(
                vm,
                secp256r1_new,
                self.context.gas_costs().secp256r1_new_gas_cost,
            ),
            SyscallSelector::SendMessageToL1 => self.execute_syscall(
                vm,
                send_message_to_l1,
                self.context.gas_costs().send_message_to_l1_gas_cost,
            ),
            SyscallSelector::StorageRead => self.execute_syscall(
                vm,
                storage_read,
                self.context.gas_costs().storage_read_gas_cost,
            ),
            SyscallSelector::StorageWrite => self.execute_syscall(
                vm,
                storage_write,
                self.context.gas_costs().storage_write_gas_cost,
            ),
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            )),
        }
    }

    pub fn get_or_allocate_execution_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> SyscallResult<Relocatable> {
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
    ) -> SyscallResult<(Relocatable, Relocatable)> {
        let l1_gas_as_felt =
            Felt::from_hex(Resource::L1Gas.to_hex()).map_err(SyscallExecutionError::from)?;
        let l2_gas_as_felt =
            Felt::from_hex(Resource::L2Gas.to_hex()).map_err(SyscallExecutionError::from)?;
        let l1_data_gas_as_felt =
            Felt::from_hex(Resource::L1DataGas.to_hex()).map_err(SyscallExecutionError::from)?;

        let l1_gas_bounds = tx_info.resource_bounds.get_l1_bounds();
        let l2_gas_bounds = tx_info.resource_bounds.get_l2_bounds();
        let mut flat_resource_bounds = vec![
            l1_gas_as_felt,
            Felt::from(l1_gas_bounds.max_amount),
            Felt::from(l1_gas_bounds.max_price_per_unit),
            l2_gas_as_felt,
            Felt::from(l2_gas_bounds.max_amount),
            Felt::from(l2_gas_bounds.max_price_per_unit),
        ];
        if let ValidResourceBounds::AllResources(AllResourceBounds { l1_data_gas, .. }) =
            tx_info.resource_bounds
        {
            flat_resource_bounds.extend(vec![
                l1_data_gas_as_felt,
                Felt::from(l1_data_gas.max_amount),
                Felt::from(l1_data_gas.max_price_per_unit),
            ])
        }

        self.allocate_data_segment(vm, &flat_resource_bounds)
    }

    fn execute_syscall<Request, Response, ExecuteCallback>(
        &mut self,
        vm: &mut VirtualMachine,
        execute_callback: ExecuteCallback,
        syscall_gas_cost: u64,
    ) -> HintExecutionResult
    where
        Request: SyscallRequest + std::fmt::Debug,
        Response: SyscallResponse + std::fmt::Debug,
        ExecuteCallback: FnOnce(
            Request,
            &mut VirtualMachine,
            &mut SyscallHintProcessor<'_>,
            &mut u64, // Remaining gas.
        ) -> SyscallResult<Response>,
    {
        // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
        let required_gas = syscall_gas_cost - self.context.gas_costs().syscall_base_gas_cost;

        let SyscallRequestWrapper { gas_counter, request } =
            SyscallRequestWrapper::<Request>::read(vm, &mut self.syscall_ptr)?;

        if gas_counter < required_gas {
            //  Out of gas failure.
            let out_of_gas_error =
                Felt::from_hex(OUT_OF_GAS_ERROR).map_err(SyscallExecutionError::from)?;
            let response: SyscallResponseWrapper<Response> =
                SyscallResponseWrapper::Failure { gas_counter, error_data: vec![out_of_gas_error] };
            response.write(vm, &mut self.syscall_ptr)?;

            return Ok(());
        }

        // Execute.
        let mut remaining_gas = gas_counter - required_gas;
        let original_response = execute_callback(request, vm, self, &mut remaining_gas);
        let response = match original_response {
            Ok(response) => {
                SyscallResponseWrapper::Success { gas_counter: remaining_gas, response }
            }
            Err(SyscallExecutionError::SyscallError { error_data: data }) => {
                SyscallResponseWrapper::Failure { gas_counter: remaining_gas, error_data: data }
            }
            Err(error) => return Err(error.into()),
        };

        response.write(vm, &mut self.syscall_ptr)?;

        Ok(())
    }

    fn read_next_syscall_selector(&mut self, vm: &mut VirtualMachine) -> SyscallResult<Felt> {
        Ok(felt_from_ptr(vm, &mut self.syscall_ptr)?)
    }

    pub fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, n: usize) {
        let syscall_count = self.syscall_counter.entry(*selector).or_default();
        *syscall_count += n;
    }

    fn increment_syscall_count(&mut self, selector: &SyscallSelector) {
        self.increment_syscall_count_by(selector, 1);
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
        let block_info = &self.context.tx_context.block_context.block_info;
        let block_timestamp = block_info.block_timestamp.0;
        let block_number = block_info.block_number.0;
        let versioned_constants = self.context.versioned_constants();
        let block_data: Vec<Felt> = if self.is_validate_mode() {
            // Round down to the nearest multiple of validate_block_number_rounding.
            let validate_block_number_rounding =
                versioned_constants.get_validate_block_number_rounding();
            let rounded_block_number =
                (block_number / validate_block_number_rounding) * validate_block_number_rounding;
            // Round down to the nearest multiple of validate_timestamp_rounding.
            let validate_timestamp_rounding = versioned_constants.get_validate_timestamp_rounding();
            let rounded_timestamp =
                (block_timestamp / validate_timestamp_rounding) * validate_timestamp_rounding;

            vec![Felt::from(rounded_block_number), Felt::from(rounded_timestamp), Felt::ZERO]
        } else {
            vec![
                Felt::from(block_number),
                Felt::from(block_timestamp),
                *block_info.sequencer_address.0.key(),
            ]
        };
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
        let tx_info = &self.context.tx_context.clone().tx_info;
        let (tx_signature_start_ptr, tx_signature_end_ptr) =
            &self.allocate_data_segment(vm, &tx_info.signature().0)?;

        let mut tx_data: Vec<MaybeRelocatable> = vec![
            tx_info.signed_version().0.into(),
            tx_info.sender_address().0.key().into(),
            max_fee_for_execution_info(tx_info).into(),
            tx_signature_start_ptr.into(),
            tx_signature_end_ptr.into(),
            (tx_info).transaction_hash().0.into(),
            Felt::from_hex(
                self.context.tx_context.block_context.chain_info.chain_id.as_hex().as_str(),
            )?
            .into(),
            (tx_info).nonce().0.into(),
        ];

        match tx_info {
            TransactionInfo::Current(context) => {
                let (tx_resource_bounds_start_ptr, tx_resource_bounds_end_ptr) =
                    &self.allocate_tx_resource_bounds_segment(vm, context)?;

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

    pub fn get_contract_storage_at(
        &mut self,
        key: StorageKey,
    ) -> SyscallResult<StorageReadResponse> {
        self.accessed_keys.insert(key);
        let value = self.state.get_storage_at(self.storage_address(), key)?;
        self.read_values.push(value);

        Ok(StorageReadResponse { value })
    }

    pub fn set_contract_storage_at(
        &mut self,
        key: StorageKey,
        value: Felt,
    ) -> SyscallResult<StorageWriteResponse> {
        let contract_address = self.storage_address();

        match self.original_values.entry(key) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(self.state.get_storage_at(contract_address, key)?);
            }
            hash_map::Entry::Occupied(_) => {}
        }

        self.accessed_keys.insert(key);
        self.state.set_storage_at(contract_address, key, value)?;

        Ok(StorageWriteResponse {})
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

impl ResourceTracker for SyscallHintProcessor<'_> {
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

impl HintProcessorLogic for SyscallHintProcessor<'_> {
    fn execute_hint(
        &mut self,
        vm: &mut VirtualMachine,
        exec_scopes: &mut ExecutionScopes,
        hint_data: &Box<dyn Any>,
        _constants: &HashMap<String, Felt>,
    ) -> HintExecutionResult {
        let hint = hint_data.downcast_ref::<Hint>().ok_or(HintError::WrongHintData)?;
        match hint {
            Hint::Core(hint) => execute_core_hint_base(vm, exec_scopes, hint),
            Hint::Starknet(hint) => self.execute_next_syscall(vm, hint),
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
    let revert_idx = syscall_handler.context.revert_infos.0.len();

    let call_info = call.execute(
        syscall_handler.state,
        syscall_handler.resources,
        syscall_handler.context,
        remaining_gas,
    )?;

    let mut raw_retdata = call_info.execution.retdata.0.clone();
    let failed = call_info.execution.failed;
    syscall_handler.inner_calls.push(call_info);
    if failed {
        syscall_handler.context.revert(revert_idx, syscall_handler.state)?;

        // Delete events and l2_to_l1_messages from the reverted call.
        let reverted_call = &mut syscall_handler.inner_calls.last_mut().unwrap();
        let mut stack: Vec<&mut CallInfo> = vec![reverted_call];
        while let Some(call_info) = stack.pop() {
            call_info.execution.events.clear();
            call_info.execution.l2_to_l1_messages.clear();
            // Add inner calls that did not fail to the stack.
            // The events and l2_to_l1_messages of the failed calls were already cleared.
            stack.extend(
                call_info.inner_calls.iter_mut().filter(|call_info| !call_info.execution.failed),
            );
        }

        raw_retdata
            .push(Felt::from_hex(ENTRYPOINT_FAILED_ERROR).map_err(SyscallExecutionError::from)?);
        return Err(SyscallExecutionError::SyscallError { error_data: raw_retdata });
    }

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

pub fn execute_library_call(
    syscall_handler: &mut SyscallHintProcessor<'_>,
    vm: &mut VirtualMachine,
    class_hash: ClassHash,
    call_to_external: bool,
    entry_point_selector: EntryPointSelector,
    calldata: Calldata,
    remaining_gas: &mut u64,
) -> SyscallResult<ReadOnlySegment> {
    let entry_point_type =
        if call_to_external { EntryPointType::External } else { EntryPointType::L1Handler };
    let entry_point = CallEntryPoint {
        class_hash: Some(class_hash),
        code_address: None,
        entry_point_type,
        entry_point_selector,
        calldata,
        // The call context remains the same in a library call.
        storage_address: syscall_handler.storage_address(),
        caller_address: syscall_handler.caller_address(),
        call_type: CallType::Delegate,
        // NOTE: this value might be overridden later on.
        initial_gas: *remaining_gas,
    };

    execute_inner_call(entry_point, vm, syscall_handler, remaining_gas).map_err(|error| match error
    {
        SyscallExecutionError::SyscallError { .. } => error,
        _ => error.as_lib_call_execution_error(
            class_hash,
            syscall_handler.storage_address(),
            entry_point_selector,
        ),
    })
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
