use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::{GasCosts, VersionedConstants};
use blockifier::execution::execution_utils::ReadOnlySegment;
use blockifier::execution::syscalls::hint_processor::{ENTRYPOINT_FAILED_ERROR, INVALID_ARGUMENT};
use blockifier::execution::syscalls::secp::SecpHintProcessor;
use blockifier::execution::syscalls::syscall_executor::SyscallExecutor;
use blockifier::execution::syscalls::vm_syscall_utils::{
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
    LibraryCallRequest,
    LibraryCallResponse,
    MetaTxV0Request,
    MetaTxV0Response,
    ReplaceClassRequest,
    ReplaceClassResponse,
    RevertData,
    SelfOrRevert,
    SendMessageToL1Request,
    SendMessageToL1Response,
    StorageReadRequest,
    StorageReadResponse,
    StorageWriteRequest,
    StorageWriteResponse,
    SyscallExecutorBaseError,
    SyscallSelector,
    TryExtractRevert,
};
use blockifier::state::state_api::StateReader;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::constants::EXECUTE_ENTRY_POINT_NAME;
use starknet_api::transaction::TransactionVersion;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::ExecutionHelperError;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    get_address_of_nested_fields_from_base_address,
    get_field_offset,
    get_size_of_cairo_struct,
    write_to_temp_segment,
    IdentifierGetter,
    VmUtilsError,
};

#[derive(Debug, thiserror::Error)]
pub enum SnosSyscallError {
    #[error(transparent)]
    ExecutionHelper(#[from] ExecutionHelperError),
    #[error("Invalid resource bounds: {0:?}")]
    InvalidResourceBounds(Vec<MaybeRelocatable>),
    #[error(transparent)]
    Math(#[from] MathError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error("Syscall revert.")]
    Revert(RevertData),
    #[error(transparent)]
    SyscallExecutorBase(#[from] SyscallExecutorBaseError),
    #[error(transparent)]
    VmUtils(#[from] VmUtilsError),
}

pub type SnosSyscallResult<T> = Result<T, SnosSyscallError>;

// Needed for custom hint implementations (in our case, syscall hints) which must comply with the
// cairo-rs API.
impl From<SnosSyscallError> for HintError {
    fn from(error: SnosSyscallError) -> Self {
        HintError::Internal(VirtualMachineError::Other(error.into()))
    }
}

impl TryExtractRevert for SnosSyscallError {
    fn try_extract_revert(self) -> SelfOrRevert<Self> {
        match self {
            Self::SyscallExecutorBase(base_error) => {
                base_error.try_extract_revert().map_original(Self::SyscallExecutorBase)
            }
            Self::Revert(revert_data) => SelfOrRevert::Revert(revert_data),
            Self::ExecutionHelper(_)
            | Self::Math(_)
            | Self::InvalidResourceBounds(_)
            | Self::VmUtils(_)
            | Self::Memory(_) => SelfOrRevert::Original(self),
        }
    }

    fn as_revert(revert_data: RevertData) -> Self {
        Self::Revert(revert_data)
    }
}

impl<S: StateReader> SyscallExecutor for SnosHintProcessor<'_, S> {
    type Error = SnosSyscallError;

    fn gas_costs(&self) -> &GasCosts {
        &self.versioned_constants().os_constants.gas_costs
    }

    fn get_secpk1_hint_processor_and_base(
        &mut self,
    ) -> (&mut SecpHintProcessor<ark_secp256k1::Config>, &mut Option<Relocatable>) {
        (
            &mut self.syscall_hint_processor.secp256k1_hint_processor,
            &mut self.syscall_hint_processor.secp_points_segment_base,
        )
    }

    fn get_secpr1_hint_processor_and_base(
        &mut self,
    ) -> (&mut SecpHintProcessor<ark_secp256r1::Config>, &mut Option<Relocatable>) {
        (
            &mut self.syscall_hint_processor.secp256r1_hint_processor,
            &mut self.syscall_hint_processor.secp_points_segment_base,
        )
    }

    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, count: usize) {
        let syscall_usage = self.syscall_hint_processor.syscall_usage.entry(*selector).or_default();
        syscall_usage.call_count += count;
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        self.syscall_hint_processor
            .get_mut_syscall_ptr()
            .expect("Syscall pointer is not initialized.")
    }

    fn update_revert_gas_with_next_remaining_gas(&mut self, _next_remaining_gas: GasAmount) {}

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> Result<CallContractResponse, Self::Error> {
        if request.function_selector == selector_from_name(EXECUTE_ENTRY_POINT_NAME) {
            return Err(handle_failure(Felt::from_hex_unchecked(INVALID_ARGUMENT)));
        }
        call_contract_helper(vm, syscall_handler, remaining_gas)
    }

    fn deploy(
        _request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> Result<DeployResponse, Self::Error> {
        let call_info_tracker = syscall_handler
            .execution_helpers_manager
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?;

        let deployed_contract_address = call_info_tracker.next_deployed_contracts_iterator()?;
        let execution = &call_info_tracker.next_inner_call()?.execution;

        *remaining_gas -= execution.gas_consumed;
        let retdata: Vec<_> = execution.retdata.0.iter().map(MaybeRelocatable::from).collect();
        let retdata_base = vm.add_temporary_segment();
        vm.load_data(retdata_base, &retdata).map_err(SyscallExecutorBaseError::from)?;
        if execution.failed {
            let revert_data = RevertData::new_temp(execution.retdata.0.clone());
            return Err(SnosSyscallError::Revert(revert_data));
        };
        Ok(DeployResponse {
            contract_address: deployed_contract_address,
            constructor_retdata: ReadOnlySegment { start_ptr: retdata_base, length: retdata.len() },
        })
    }

    fn emit_event(
        _request: EmitEventRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<EmitEventResponse, Self::Error> {
        Ok(EmitEventResponse {})
    }

    fn get_block_hash(
        request: GetBlockHashRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<GetBlockHashResponse, Self::Error> {
        let block_number = request.block_number;
        let execution_helper = syscall_handler.get_mut_current_execution_helper()?;
        let diff = execution_helper.os_block_input.block_info.block_number.0 - block_number.0;
        assert!(diff >= STORED_BLOCK_HASH_BUFFER, "Block number out of range {diff}.");
        let block_hash = execution_helper
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?
            .next_execute_code_block_hash_read()?;

        Ok(GetBlockHashResponse { block_hash: *block_hash })
    }

    fn get_class_hash_at(
        _request: GetClassHashAtRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<GetClassHashAtResponse, Self::Error> {
        let class_hash = syscall_handler
            .execution_helpers_manager
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?
            .next_execute_code_class_hash_read()?;
        Ok(*class_hash)
    }

    fn get_execution_info(
        _request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<GetExecutionInfoResponse, Self::Error> {
        let call_info_tracker = syscall_handler.get_current_call_info_tracker()?;
        let original_execution_info_ptr = call_info_tracker.execution_info_ptr;
        let class_hash =
            call_info_tracker.call_info.call.class_hash.expect("No class hash was set.");
        let tx_version =
            syscall_handler.get_execution_info_nested_field_value(&["tx_info", "version"], vm)?;

        let os_constants = &syscall_handler.versioned_constants().os_constants;
        // Check if we should exclude L1 data gas for this class hash.
        let should_exclude_l1_data_gas = tx_version == TransactionVersion::THREE.0
            && os_constants.data_gas_accounts.contains(&class_hash);
        // Check if we should return version = 1.
        let tip = syscall_handler.get_execution_info_nested_field_value(&["tx_info", "tip"], vm)?;
        let should_replace_to_v1 = tx_version == TransactionVersion::THREE.0
            && os_constants.v1_bound_accounts_cairo1.contains(&class_hash)
            && tip <= Felt::from(os_constants.v1_bound_accounts_max_tip.0);

        // Allocate or return the original execution info segment.
        let execution_info_ptr = allocate_or_return_execution_info_segment(
            original_execution_info_ptr,
            should_exclude_l1_data_gas,
            should_replace_to_v1,
            vm,
            syscall_handler.program,
        )?;
        Ok(GetExecutionInfoResponse { execution_info_ptr })
    }

    fn library_call(
        _request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> Result<LibraryCallResponse, Self::Error> {
        call_contract_helper(vm, syscall_handler, remaining_gas)
    }

    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> Result<MetaTxV0Response, Self::Error> {
        if request.entry_point_selector != selector_from_name(EXECUTE_ENTRY_POINT_NAME) {
            return Err(handle_failure(Felt::from_hex_unchecked(INVALID_ARGUMENT)));
        }
        call_contract_helper(vm, syscall_handler, remaining_gas)
    }

    fn replace_class(
        _request: ReplaceClassRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<ReplaceClassResponse, Self::Error> {
        Ok(ReplaceClassResponse {})
    }

    fn send_message_to_l1(
        _request: SendMessageToL1Request,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<SendMessageToL1Response, Self::Error> {
        Ok(SendMessageToL1Response {})
    }

    fn storage_read(
        request: StorageReadRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<StorageReadResponse, Self::Error> {
        assert_eq!(request.address_domain, Felt::ZERO);
        let value = *syscall_handler
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?
            .next_execute_code_read()?;

        Ok(StorageReadResponse { value })
    }

    fn storage_write(
        _request: StorageWriteRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> Result<StorageWriteResponse, Self::Error> {
        Ok(StorageWriteResponse {})
    }

    fn versioned_constants(&self) -> &VersionedConstants {
        VersionedConstants::latest_constants()
    }

    fn write_sha256_state(
        &mut self,
        state: &[MaybeRelocatable],
        vm: &mut VirtualMachine,
    ) -> Result<Relocatable, Self::Error> {
        let segment_start =
            self.syscall_hint_processor.sha256_segment.expect("SHA256 segment must be set in OS.");
        let entries_offset =
            get_size_of_cairo_struct(CairoStruct::Sha256ProcessBlock, self.program)?
                * self.syscall_hint_processor.sha256_block_count;
        let out_state_offset =
            get_field_offset(CairoStruct::Sha256ProcessBlock, "out_state", self.program)?;
        let total_offset = entries_offset + out_state_offset;
        let state_start = (segment_start + total_offset)?;
        vm.load_data(state_start, state)?;

        // Increment the block count for the next call.
        self.syscall_hint_processor.sha256_block_count += 1;
        Ok(state_start)
    }
}

fn allocate_or_return_execution_info_segment<IG: IdentifierGetter>(
    original_ptr: Relocatable,
    should_exclude_l1_data_gas: bool,
    should_replace_to_v1: bool,
    vm: &mut VirtualMachine,
    identifier_getter: &IG,
) -> Result<Relocatable, SnosSyscallError> {
    if !should_replace_to_v1 && !should_exclude_l1_data_gas {
        // No need to replace anything - return the original pointer.
        return Ok(original_ptr);
    }

    let replaced_execution_info = vm.add_memory_segment();
    let tx_info_ptr = vm.get_relocatable(get_address_of_nested_fields_from_base_address(
        original_ptr,
        CairoStruct::ExecutionInfo,
        vm,
        &["tx_info"],
        identifier_getter,
    )?)?;
    let tx_info_size = get_size_of_cairo_struct(CairoStruct::TxInfo, identifier_getter)?;
    let mut flattened_tx_info = vm.get_continuous_range(tx_info_ptr, tx_info_size)?;
    if should_replace_to_v1 {
        let version_offset = get_field_offset(CairoStruct::TxInfo, "version", identifier_getter)?;
        flattened_tx_info[version_offset] = TransactionVersion::ONE.0.into();
    }
    if should_exclude_l1_data_gas {
        let resource_bounds_start_offset =
            get_field_offset(CairoStruct::TxInfo, "resource_bounds_start", identifier_getter)?;
        let resource_bounds_end_offset =
            get_field_offset(CairoStruct::TxInfo, "resource_bounds_end", identifier_getter)?;

        let resource_bounds_start =
            vm.get_relocatable((tx_info_ptr + resource_bounds_start_offset)?)?;
        let resource_bounds_end =
            vm.get_relocatable((tx_info_ptr + resource_bounds_end_offset)?)?;

        let resource_bounds_size =
            get_size_of_cairo_struct(CairoStruct::ResourceBounds, identifier_getter)?;
        // Verify all resource bounds are present.
        assert!(resource_bounds_size != 0);
        assert!(
            (resource_bounds_end.offset - resource_bounds_start.offset) % resource_bounds_size == 0,
            "Resource bounds segment length is not a multiple of resource bounds size."
        );
        if (resource_bounds_end.offset - resource_bounds_start.offset) / resource_bounds_size != 3 {
            return Err(SnosSyscallError::InvalidResourceBounds(vm.get_continuous_range(
                resource_bounds_start,
                resource_bounds_end.offset - resource_bounds_start.offset,
            )?));
        }
        // Subtract the size of a resource from the end to exclude the last resource.
        flattened_tx_info[resource_bounds_end_offset] =
            (resource_bounds_end - resource_bounds_size)?.into();
    }
    let mut flattened_execution_info = vm.get_continuous_range(
        original_ptr,
        get_size_of_cairo_struct(CairoStruct::ExecutionInfo, identifier_getter)?,
    )?;
    let tx_info_offset =
        get_field_offset(CairoStruct::ExecutionInfo, "tx_info", identifier_getter)?;
    let replaced_tx_info = vm.gen_arg(&flattened_tx_info)?;
    flattened_execution_info[tx_info_offset] = replaced_tx_info;
    vm.load_data(replaced_execution_info, &flattened_execution_info)?;
    Ok(replaced_execution_info)
}

fn call_contract_helper(
    vm: &mut VirtualMachine,
    syscall_handler: &mut SnosHintProcessor<'_, impl StateReader>,
    remaining_gas: &mut u64,
) -> SnosSyscallResult<CallContractResponse> {
    let next_call_execution = syscall_handler.get_next_call_execution()?;
    *remaining_gas -= next_call_execution.gas_consumed;
    let retdata = &next_call_execution.retdata.0;
    let revert_error_code = Felt::from_hex_unchecked(ENTRYPOINT_FAILED_ERROR);
    if next_call_execution.failed {
        let mut retdata = retdata.clone();
        retdata.push(revert_error_code);
        let revert_data = RevertData::new_temp(retdata);
        return Err(SnosSyscallError::Revert(revert_data));
    };

    Ok(CallContractResponse { segment: write_to_temp_segment(retdata, vm)? })
}

/// Returns an revert error with the given error code which will be written to a normal segment.
fn handle_failure(error_code: Felt) -> SnosSyscallError {
    SnosSyscallError::Revert(RevertData::new_normal(vec![error_code]))
}
