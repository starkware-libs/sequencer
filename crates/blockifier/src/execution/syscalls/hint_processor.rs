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
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionVersion;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::{Felt, FromStrError};
use thiserror::Error;

use crate::abi::sierra_types::SierraTypeError;
use crate::blockifier_versioned_constants::{GasCosts, SyscallGasCost};
use crate::execution::common_hints::{ExecutionMode, HintExecutionResult};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{
    CallEntryPoint,
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
use crate::execution::syscalls::syscall_base::SyscallHandlerBase;
use crate::execution::syscalls::{
    call_contract,
    deploy,
    emit_event,
    get_block_hash,
    get_class_hash_at,
    get_execution_info,
    keccak,
    library_call,
    meta_tx_v0,
    replace_class,
    send_message_to_l1,
    sha_256_process_block,
    storage_read,
    storage_write,
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
use crate::utils::u64_from_usize;

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

pub struct ResourceAsFelts {
    pub resource_name: Felt,
    pub max_amount: Felt,
    pub max_price_per_unit: Felt,
}

impl ResourceAsFelts {
    pub fn new(resource: Resource, resource_bounds: &ResourceBounds) -> SyscallResult<Self> {
        Ok(Self {
            resource_name: Felt::from_hex(resource.to_hex())
                .map_err(SyscallExecutionError::from)?,
            max_amount: Felt::from(resource_bounds.max_amount),
            max_price_per_unit: Felt::from(resource_bounds.max_price_per_unit),
        })
    }

    pub fn flatten(self) -> Vec<Felt> {
        vec![self.resource_name, self.max_amount, self.max_price_per_unit]
    }
}

pub fn valid_resource_bounds_as_felts(
    resource_bounds: &ValidResourceBounds,
    exclude_l1_data_gas: bool,
) -> SyscallResult<Vec<ResourceAsFelts>> {
    let mut resource_bounds_vec: Vec<_> = vec![
        ResourceAsFelts::new(Resource::L1Gas, &resource_bounds.get_l1_bounds())?,
        ResourceAsFelts::new(Resource::L2Gas, &resource_bounds.get_l2_bounds())?,
    ];
    if exclude_l1_data_gas {
        return Ok(resource_bounds_vec);
    }
    if let ValidResourceBounds::AllResources(AllResourceBounds { l1_data_gas, .. }) =
        resource_bounds
    {
        resource_bounds_vec.push(ResourceAsFelts::new(Resource::L1DataGas, l1_data_gas)?)
    }
    Ok(resource_bounds_vec)
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

        match selector {
            SyscallSelector::CallContract => {
                self.execute_syscall(vm, call_contract, self.gas_costs().syscalls.call_contract)
            }
            SyscallSelector::Deploy => {
                self.execute_syscall(vm, deploy, self.gas_costs().syscalls.deploy)
            }
            SyscallSelector::EmitEvent => {
                self.execute_syscall(vm, emit_event, self.gas_costs().syscalls.emit_event)
            }
            SyscallSelector::GetBlockHash => {
                self.execute_syscall(vm, get_block_hash, self.gas_costs().syscalls.get_block_hash)
            }
            SyscallSelector::GetClassHashAt => self.execute_syscall(
                vm,
                get_class_hash_at,
                self.gas_costs().syscalls.get_class_hash_at,
            ),
            SyscallSelector::GetExecutionInfo => self.execute_syscall(
                vm,
                get_execution_info,
                self.gas_costs().syscalls.get_execution_info,
            ),
            SyscallSelector::Keccak => {
                self.execute_syscall(vm, keccak, self.gas_costs().syscalls.keccak)
            }
            SyscallSelector::Sha256ProcessBlock => self.execute_syscall(
                vm,
                sha_256_process_block,
                self.gas_costs().syscalls.sha256_process_block,
            ),
            SyscallSelector::LibraryCall => {
                self.execute_syscall(vm, library_call, self.gas_costs().syscalls.library_call)
            }
            SyscallSelector::MetaTxV0 => {
                self.execute_syscall(vm, meta_tx_v0, self.gas_costs().syscalls.meta_tx_v0)
            }
            SyscallSelector::ReplaceClass => {
                self.execute_syscall(vm, replace_class, self.gas_costs().syscalls.replace_class)
            }
            SyscallSelector::Secp256k1Add => {
                self.execute_syscall(vm, secp256k1_add, self.gas_costs().syscalls.secp256k1_add)
            }
            SyscallSelector::Secp256k1GetPointFromX => self.execute_syscall(
                vm,
                secp256k1_get_point_from_x,
                self.gas_costs().syscalls.secp256k1_get_point_from_x,
            ),
            SyscallSelector::Secp256k1GetXy => self.execute_syscall(
                vm,
                secp256k1_get_xy,
                self.gas_costs().syscalls.secp256k1_get_xy,
            ),
            SyscallSelector::Secp256k1Mul => {
                self.execute_syscall(vm, secp256k1_mul, self.gas_costs().syscalls.secp256k1_mul)
            }
            SyscallSelector::Secp256k1New => {
                self.execute_syscall(vm, secp256k1_new, self.gas_costs().syscalls.secp256k1_new)
            }
            SyscallSelector::Secp256r1Add => {
                self.execute_syscall(vm, secp256r1_add, self.gas_costs().syscalls.secp256r1_add)
            }
            SyscallSelector::Secp256r1GetPointFromX => self.execute_syscall(
                vm,
                secp256r1_get_point_from_x,
                self.gas_costs().syscalls.secp256r1_get_point_from_x,
            ),
            SyscallSelector::Secp256r1GetXy => self.execute_syscall(
                vm,
                secp256r1_get_xy,
                self.gas_costs().syscalls.secp256r1_get_xy,
            ),
            SyscallSelector::Secp256r1Mul => {
                self.execute_syscall(vm, secp256r1_mul, self.gas_costs().syscalls.secp256r1_mul)
            }
            SyscallSelector::Secp256r1New => {
                self.execute_syscall(vm, secp256r1_new, self.gas_costs().syscalls.secp256r1_new)
            }
            SyscallSelector::SendMessageToL1 => self.execute_syscall(
                vm,
                send_message_to_l1,
                self.gas_costs().syscalls.send_message_to_l1,
            ),
            SyscallSelector::StorageRead => {
                self.execute_syscall(vm, storage_read, self.gas_costs().syscalls.storage_read)
            }
            SyscallSelector::StorageWrite => {
                self.execute_syscall(vm, storage_write, self.gas_costs().syscalls.storage_write)
            }
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            )),
        }
    }

    pub fn get_or_allocate_execution_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
    ) -> SyscallResult<Relocatable> {
        let returned_version = self.base.tx_version_for_get_execution_info();
        let original_version = self.base.context.tx_context.tx_info.signed_version();
        let exclude_l1_data_gas = self.base.exclude_l1_data_gas();

        // If the transaction version was overridden, `self.execution_info_ptr` cannot be used.
        if returned_version != original_version {
            return self.allocate_execution_info_segment(vm, returned_version, exclude_l1_data_gas);
        }

        match self.execution_info_ptr {
            Some(execution_info_ptr) => Ok(execution_info_ptr),
            None => {
                let execution_info_ptr = self.allocate_execution_info_segment(
                    vm,
                    original_version,
                    exclude_l1_data_gas,
                )?;
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

    fn execute_syscall<Request, Response, ExecuteCallback>(
        &mut self,
        vm: &mut VirtualMachine,
        execute_callback: ExecuteCallback,
        syscall_gas_cost: SyscallGasCost,
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
        let SyscallRequestWrapper { gas_counter, request } =
            SyscallRequestWrapper::<Request>::read(vm, &mut self.syscall_ptr)?;

        let syscall_gas_cost =
            syscall_gas_cost.get_syscall_cost(u64_from_usize(request.get_linear_factor_length()));
        let syscall_base_cost = self.base.context.gas_costs().base.syscall_base_gas_cost;

        // Sanity check for preventing underflow.
        assert!(
            syscall_gas_cost >= syscall_base_cost,
            "Syscall gas cost must be greater than base syscall gas cost"
        );

        // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
        let required_gas = syscall_gas_cost - syscall_base_cost;

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

        // To support sierra gas charge for blockifier revert flow, we track the remaining gas left
        // before executing a syscall if the current tracked resource is gas.
        // 1. If the syscall does not run Cairo code (i.e. not library call, not call contract, and
        //    not a deploy), any failure will not run in the OS, so no need to charge - the value
        //    before entering the callback is good enough to charge.
        // 2. If the syscall runs Cairo code, but the tracked resource is steps (and not gas), the
        //    additional charge of reverted cairo steps will cover the inner cost, and the outer
        //    cost we track here will be the additional reverted gas.
        // 3. If the syscall runs Cairo code and the tracked resource is gas, either the inner
        //    failure will be a Cairo1 revert (and the gas consumed on the call info will override
        //    the current tracked value), or we will pass through another syscall before failing -
        //    and by induction (we will reach this point again), the gas will be charged correctly.
        self.base.context.update_revert_gas_with_next_remaining_gas(GasAmount(remaining_gas));

        let original_response = execute_callback(request, vm, self, &mut remaining_gas);
        let response = match original_response {
            Ok(response) => {
                SyscallResponseWrapper::Success { gas_counter: remaining_gas, response }
            }
            Err(SyscallExecutionError::Revert { error_data: data }) => {
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
        tx_version_override: TransactionVersion,
        exclude_l1_data_gas: bool,
    ) -> SyscallResult<Relocatable> {
        let block_info_ptr = self.allocate_block_info_segment(vm)?;
        let tx_info_ptr =
            self.allocate_tx_info_segment(vm, tx_version_override, exclude_l1_data_gas)?;

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

    fn allocate_tx_info_segment(
        &mut self,
        vm: &mut VirtualMachine,
        tx_version_override: TransactionVersion,
        exclude_l1_data_gas: bool,
    ) -> SyscallResult<Relocatable> {
        let tx_info = &self.base.context.tx_context.clone().tx_info;
        let (tx_signature_start_ptr, tx_signature_end_ptr) =
            &self.allocate_data_segment(vm, &tx_info.signature().0)?;

        let mut tx_data: Vec<MaybeRelocatable> = vec![
            tx_version_override.0.into(),
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
                let (tx_resource_bounds_start_ptr, tx_resource_bounds_end_ptr) =
                    &self.allocate_tx_resource_bounds_segment(vm, context, exclude_l1_data_gas)?;

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
