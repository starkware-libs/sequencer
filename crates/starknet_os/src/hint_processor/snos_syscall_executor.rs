use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::execution::execution_utils::ReadOnlySegment;
use blockifier::execution::syscalls::hint_processor::SyscallExecutionError;
use blockifier::execution::syscalls::secp::SecpHintProcessor;
use blockifier::execution::syscalls::syscall_base::SyscallResult;
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
    SelfOrRevert,
    SendMessageToL1Request,
    SendMessageToL1Response,
    Sha256ProcessBlockRequest,
    Sha256ProcessBlockResponse,
    StorageReadRequest,
    StorageReadResponse,
    StorageWriteRequest,
    StorageWriteResponse,
    SyscallExecutorBaseError,
    SyscallSelector,
    TryExtractRevert,
};
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::TransactionVersion;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::ExecutionHelperError;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    get_address_of_nested_fields_from_base_address,
    get_field_offset,
    get_size_of_cairo_struct,
    IdentifierGetter,
};

#[derive(Debug, thiserror::Error)]
pub enum SnosSyscallError {
    #[error(transparent)]
    SyscallExecutorBase(#[from] SyscallExecutorBaseError),
}

impl TryExtractRevert for SnosSyscallError {
    fn try_extract_revert(self) -> SelfOrRevert<Self> {
        match self {
            SnosSyscallError::SyscallExecutorBase(base_error) => {
                base_error.try_extract_revert().map_original(Self::SyscallExecutorBase)
            }
        }
    }

    fn as_revert(error_data: Vec<Felt>) -> Self {
        SyscallExecutorBaseError::Revert { error_data }.into()
    }
}

#[allow(unused_variables)]
impl<S: StateReader> SyscallExecutor for SnosHintProcessor<'_, S> {
    type Error = SnosSyscallError;

    fn get_keccak_round_cost_base_syscall_cost(&self) -> u64 {
        todo!()
    }

    fn get_secpk1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256k1::Config> {
        &mut self.syscall_hint_processor.secp256k1_hint_processor
    }

    fn get_secpr1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256r1::Config> {
        &mut self.syscall_hint_processor.secp256r1_hint_processor
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

    fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount) {}

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse> {
        // TODO(Tzahi): Change `expect`s to regular errors once the syscall trait has an associated
        // error type.
        let call_tracker = syscall_handler
            .execution_helpers_manager
            .get_mut_current_execution_helper()
            .expect("No current execution helper")
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()
            .expect("No current tx execution info")
            .call_info_tracker
            .as_mut()
            .expect("No call info tracker found");

        let next_call_execution = &call_tracker
            .inner_calls_iterator
            .next()
            .ok_or(ExecutionHelperError::MissingCallInfo)
            .expect("Missing call info")
            .execution;

        *remaining_gas -= next_call_execution.gas_consumed;
        let ret_data = &next_call_execution.retdata.0;

        if next_call_execution.failed {
            return Err(SyscallExecutionError::Revert { error_data: ret_data.clone() });
        };

        let relocatable_ret_data: Vec<MaybeRelocatable> =
            ret_data.iter().map(|&x| MaybeRelocatable::from(x)).collect();

        let retdata_segment_start_ptr = vm.add_temporary_segment();
        vm.load_data(retdata_segment_start_ptr, &relocatable_ret_data)?;

        Ok(CallContractResponse {
            segment: ReadOnlySegment {
                start_ptr: retdata_segment_start_ptr,
                length: relocatable_ret_data.len(),
            },
        })
    }

    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse> {
        todo!()
    }

    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse> {
        todo!()
    }

    fn get_block_hash(
        request: GetBlockHashRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse> {
        todo!()
    }

    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse> {
        todo!()
    }

    fn get_execution_info(
        request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse> {
        // TODO(Nimrod): Handle errors correctly.
        let call_info_tracker = syscall_handler
            .get_current_execution_helper()
            .unwrap()
            .tx_execution_iter
            .get_tx_execution_info_ref()
            .unwrap()
            .get_call_info_tracker()
            .unwrap();
        let original_execution_info_ptr = call_info_tracker.execution_info_ptr;
        let class_hash = call_info_tracker.call_info.call.class_hash.unwrap();

        // We assume that the OS accepts only V3 txs.
        let versioned_constants = syscall_handler.versioned_constants();
        // Check if we should exclude L1 data gas for this class hash.
        let should_exclude_l1_data_gas =
            versioned_constants.os_constants.data_gas_accounts.contains(&class_hash);
        // Check if we should return version = 1.
        let tip = vm
            .get_integer(
                get_address_of_nested_fields_from_base_address(
                    original_execution_info_ptr,
                    CairoStruct::ExecutionInfo,
                    vm,
                    &["tx_info", "tip"],
                    syscall_handler.os_program,
                )
                .unwrap(),
            )
            .unwrap();
        let should_replace_to_v1 = syscall_handler
            .versioned_constants()
            .os_constants
            .v1_bound_accounts_cairo0
            .contains(&class_hash)
            && tip.into_owned()
                <= Felt::from(versioned_constants.os_constants.v1_bound_accounts_max_tip.0);

        // Allocate or return the original execution info segment.
        let execution_info_ptr = allocate_or_return_execution_info_segment(
            original_execution_info_ptr,
            should_exclude_l1_data_gas,
            should_replace_to_v1,
            vm,
            syscall_handler.os_program,
        );
        Ok(GetExecutionInfoResponse { execution_info_ptr })
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse> {
        todo!()
    }

    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response> {
        todo!()
    }

    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse> {
        todo!()
    }

    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse> {
        todo!()
    }

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response> {
        todo!()
    }

    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse> {
        // TODO(Tzahi): Change `expect`s to regular errors once the syscall trait has an associated
        // error type.
        assert_eq!(request.address_domain, Felt::ZERO);
        let value = *syscall_handler
            .get_mut_current_execution_helper()
            .expect("No current execution helper")
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()
            .expect("No current tx execution info")
            .get_mut_call_info_tracker()
            .expect("No call info tracker found")
            .execute_code_read_iterator
            .next()
            .expect("Missing hint for read_storage");

        Ok(StorageReadResponse { value })
    }

    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse> {
        Ok(StorageWriteResponse {})
    }

    fn versioned_constants(&self) -> &VersionedConstants {
        VersionedConstants::latest_constants()
    }
}

fn allocate_or_return_execution_info_segment<IG: IdentifierGetter>(
    original_ptr: Relocatable,
    should_exclude_l1_data_gas: bool,
    should_replace_to_v1: bool,
    vm: &mut VirtualMachine,
    identifier_getter: &IG,
) -> Relocatable {
    // TODO(Nimrod): Handle errors correctly.
    if !should_replace_to_v1 && !should_exclude_l1_data_gas {
        // No need to replace anything - return the original pointer.
        return original_ptr;
    }

    let replaced_execution_info = vm.add_memory_segment();
    let tx_info_ptr = vm
        .get_relocatable(
            get_address_of_nested_fields_from_base_address(
                original_ptr,
                CairoStruct::ExecutionInfo,
                vm,
                &["tx_info"],
                identifier_getter,
            )
            .unwrap(),
        )
        .unwrap();
    let block_info_ptr = vm
        .get_relocatable(
            get_address_of_nested_fields_from_base_address(
                original_ptr,
                CairoStruct::ExecutionInfo,
                vm,
                &["block_info"],
                identifier_getter,
            )
            .unwrap(),
        )
        .unwrap();
    let tx_info_size = get_size_of_cairo_struct(CairoStruct::TxInfo, identifier_getter).unwrap();
    let mut flattened_tx_info = vm.get_continuous_range(tx_info_ptr, tx_info_size).unwrap();
    if should_replace_to_v1 {
        let version_offset =
            get_field_offset(CairoStruct::TxInfo, "version", identifier_getter).unwrap();
        flattened_tx_info[version_offset] = TransactionVersion::ONE.0.into();
    }
    if should_exclude_l1_data_gas {
        let resource_bounds_end_offset =
            get_field_offset(CairoStruct::TxInfo, "resource_bounds_end", identifier_getter)
                .unwrap();

        let resource_bounds_end =
            vm.get_relocatable((tx_info_ptr + resource_bounds_end_offset).unwrap()).unwrap();

        // Subtract the size of a resource last resource bound.
        flattened_tx_info[resource_bounds_end_offset] = (resource_bounds_end
            - get_size_of_cairo_struct(CairoStruct::ResourceBounds, identifier_getter).unwrap())
        .unwrap()
        .into();
    }
    let mut flattened_execution_info = vm
        .get_continuous_range(
            original_ptr,
            get_size_of_cairo_struct(CairoStruct::ExecutionInfo, identifier_getter).unwrap(),
        )
        .unwrap();
    // Allocate a new segment for the block info to have memory consistency with python, although
    // it's not required.
    let block_info_offset =
        get_field_offset(CairoStruct::ExecutionInfo, "block_info", identifier_getter).unwrap();
    let flattened_block_info = vm
        .get_continuous_range(
            block_info_ptr,
            get_size_of_cairo_struct(CairoStruct::BlockInfo, identifier_getter).unwrap(),
        )
        .unwrap();
    let replaced_block_info = vm.gen_arg(&flattened_block_info).unwrap();
    let tx_info_offset =
        get_field_offset(CairoStruct::ExecutionInfo, "tx_info", identifier_getter).unwrap();
    let replaced_tx_info = vm.gen_arg(&flattened_tx_info).unwrap();
    flattened_execution_info[tx_info_offset] = replaced_tx_info;
    flattened_execution_info[block_info_offset] = replaced_block_info;
    vm.load_data(replaced_execution_info, &flattened_execution_info).unwrap();
    replaced_execution_info
}
