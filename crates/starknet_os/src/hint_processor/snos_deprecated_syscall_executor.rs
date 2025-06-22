use blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::{
    DeprecatedSyscallExecutor,
    DeprecatedSyscallExecutorBaseError,
};
use blockifier::execution::deprecated_syscalls::{
    CallContractRequest,
    CallContractResponse,
    DelegateCallRequest,
    DelegateCallResponse,
    DeployRequest,
    DeployResponse,
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
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::execution::execution_utils::ReadOnlySegment;
use blockifier::execution::syscalls::syscall_executor::SyscallExecutor;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::transaction::TransactionVersion;
use starknet_api::StarknetApiError;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::{CallInfoTracker, ExecutionHelperError};
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    get_address_of_nested_fields_from_base_address,
    get_field_offset,
    get_size_of_cairo_struct,
    write_to_temp_segment,
    VmUtilsError,
};

#[derive(Debug, thiserror::Error)]
pub enum DeprecatedSnosSyscallError {
    #[error(transparent)]
    ExecutionHelper(#[from] ExecutionHelperError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    SyscallExecutorBase(#[from] DeprecatedSyscallExecutorBaseError),
    #[error(transparent)]
    VmUtils(#[from] VmUtilsError),
}

#[derive(Debug)]
pub enum CallRequest {
    CallContract(CallContractRequest),
    DelegateCall(CallContractRequest),
    DelegateL1Handler(CallContractRequest),
    LibraryCall(LibraryCallRequest),
    LibraryCallL1Handler(LibraryCallRequest),
}

impl<'a, S: StateReader> SnosHintProcessor<'a, S> {
    fn _call_contract(
        request: CallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<CallContractResponse, DeprecatedSnosSyscallError> {
        let next_call_execution = syscall_handler.get_next_call_execution()?;

        let ret_data = &next_call_execution.retdata.0;
        if next_call_execution.failed {
            // A transaction with a failed Cairo0 call should not reach the OS.
            panic!(
                "Unexpected reverted call (Cairo0 call failed, but reached the OS). \nRequest: \
                 {request:?} \nReturned data: {ret_data:?}",
            );
        };

        Ok(CallContractResponse { segment: write_to_temp_segment(ret_data, vm)? })
    }

    fn get_mut_call_info_tracker(
        &mut self,
    ) -> Result<&mut CallInfoTracker<'a>, DeprecatedSnosSyscallError> {
        Ok(self
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?)
    }

    fn get_call_entry_point(&mut self) -> Result<&CallEntryPoint, DeprecatedSnosSyscallError> {
        Ok(&self.get_mut_call_info_tracker()?.call_info.call)
    }

    fn _get_tx_info_ptr(
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<Relocatable, DeprecatedSnosSyscallError> {
        let call_info_tracker = syscall_handler.get_current_call_info_tracker()?;
        let original_tx_info_start_ptr = call_info_tracker.deprecated_tx_info_ptr;
        let class_hash =
            call_info_tracker.call_info.call.class_hash.expect("No class hash was set.");
        let tx_version = *vm.get_integer(get_address_of_nested_fields_from_base_address(
            original_tx_info_start_ptr,
            CairoStruct::TxInfo,
            vm,
            &["version"],
            syscall_handler.program,
        )?)?;
        let os_constants = &syscall_handler.versioned_constants().os_constants;
        // Check if we should return version = 1.
        let tip = syscall_handler.get_execution_info_nested_field_value(&["tx_info", "tip"], vm)?;

        let should_replace_to_v1 = tx_version == TransactionVersion::THREE.0
            && os_constants.v1_bound_accounts_cairo0.contains(&class_hash)
            && tip <= Felt::from(os_constants.v1_bound_accounts_max_tip.0);

        if should_replace_to_v1 {
            // Deal with version bound accounts.
            let replaced_tx_info = vm.add_memory_segment();
            let tx_info_size =
                get_size_of_cairo_struct(CairoStruct::DeprecatedTxInfo, syscall_handler.program)?;
            let mut flattened_tx_info =
                vm.get_continuous_range(original_tx_info_start_ptr, tx_info_size)?;
            let version_offset = get_field_offset(
                CairoStruct::DeprecatedTxInfo,
                "version",
                syscall_handler.program,
            )?;
            // Replace the version field with 1.
            flattened_tx_info[version_offset] = TransactionVersion::ONE.0.into();
            vm.load_data(replaced_tx_info, &flattened_tx_info)?;
            Ok(replaced_tx_info)
        } else {
            Ok(original_tx_info_start_ptr)
        }
    }
}

impl<S: StateReader> DeprecatedSyscallExecutor for SnosHintProcessor<'_, S> {
    type Error = DeprecatedSnosSyscallError;

    fn increment_syscall_count(&mut self, selector: &DeprecatedSyscallSelector) {
        let current_execution_helper =
            self.get_mut_current_execution_helper().expect("Current execution helper must be set.");
        current_execution_helper
            .deprecated_syscall_hint_processor
            .syscalls_usage
            .entry(*selector)
            .or_default()
            .increment_call_count();
    }

    fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> Result<(), Self::Error> {
        let expected_ptr = self
            .get_current_execution_helper()?
            .deprecated_syscall_hint_processor
            .syscall_ptr
            .expect("Syscall must be set at this point.");
        if actual_ptr != expected_ptr {
            return Err(DeprecatedSyscallExecutorBaseError::BadSyscallPointer {
                expected_ptr,
                actual_ptr,
            })?;
        }
        Ok(())
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        let current_execution_helper =
            self.get_mut_current_execution_helper().expect("Current execution helper must be set.");
        current_execution_helper
            .deprecated_syscall_hint_processor
            .syscall_ptr
            .as_mut()
            .expect("Syscall pointer must be set when executing syscall.")
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<CallContractResponse, Self::Error> {
        Self::_call_contract(CallRequest::CallContract(request), vm, syscall_handler)
    }

    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::DelegateCall(request), vm, syscall_handler)
    }

    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::DelegateL1Handler(request), vm, syscall_handler)
    }

    fn deploy(
        _request: DeployRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DeployResponse, Self::Error> {
        let call_info_tracker = syscall_handler
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?;
        call_info_tracker.next_inner_call()?;
        let contract_address = call_info_tracker.next_deployed_contracts_iterator()?;
        Ok(DeployResponse { contract_address })
    }

    fn emit_event(
        _request: EmitEventRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
    ) -> Result<EmitEventResponse, Self::Error> {
        Ok(EmitEventResponse {})
    }

    fn get_block_number(
        _request: GetBlockNumberRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockNumberResponse, Self::Error> {
        let block_number_as_felt = syscall_handler
            .get_execution_info_nested_field_value(&["block_info", "block_number"], vm)?;
        let block_number = BlockNumber(
            u64::try_from(block_number_as_felt).expect("Block number is expected to fit in u64."),
        );
        Ok(GetBlockNumberResponse { block_number })
    }

    fn get_block_timestamp(
        _request: GetBlockTimestampRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockTimestampResponse, Self::Error> {
        let block_timestamp_as_felt = syscall_handler
            .get_execution_info_nested_field_value(&["block_info", "block_timestamp"], vm)?;
        let block_timestamp = BlockTimestamp(
            u64::try_from(block_timestamp_as_felt)
                .expect("Block timestamp is expected to fit in u64."),
        );
        Ok(GetBlockTimestampResponse { block_timestamp })
    }

    fn get_caller_address(
        _request: GetCallerAddressRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetCallerAddressResponse, Self::Error> {
        Ok(GetCallerAddressResponse {
            address: syscall_handler.get_call_entry_point()?.caller_address,
        })
    }

    fn get_contract_address(
        _request: GetContractAddressRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetContractAddressResponse, Self::Error> {
        Ok(GetContractAddressResponse {
            address: syscall_handler.get_call_entry_point()?.storage_address,
        })
    }

    fn get_sequencer_address(
        _request: GetSequencerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetSequencerAddressResponse, Self::Error> {
        let sequencer_address = syscall_handler
            .get_execution_info_nested_field_value(&["block_info", "sequencer_address"], vm)?
            .try_into()?;
        Ok(GetSequencerAddressResponse { address: sequencer_address })
    }

    fn get_tx_info(
        _request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxInfoResponse, Self::Error> {
        let tx_info_start_ptr = Self::_get_tx_info_ptr(vm, syscall_handler)?;
        Ok(GetTxInfoResponse { tx_info_start_ptr })
    }

    fn get_tx_signature(
        _request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxSignatureResponse, Self::Error> {
        let tx_info_start_ptr = Self::_get_tx_info_ptr(vm, syscall_handler)?;
        let tx_signature_start_ptr =
            vm.get_relocatable(get_address_of_nested_fields_from_base_address(
                tx_info_start_ptr,
                CairoStruct::DeprecatedTxInfo,
                vm,
                &["signature"],
                syscall_handler.program,
            )?)?;
        let tx_signature_len = *vm.get_integer(get_address_of_nested_fields_from_base_address(
            tx_info_start_ptr,
            CairoStruct::DeprecatedTxInfo,
            vm,
            &["signature_len"],
            syscall_handler.program,
        )?)?;
        Ok(GetTxSignatureResponse {
            segment: ReadOnlySegment {
                start_ptr: tx_signature_start_ptr,
                length: felt_to_usize(&tx_signature_len)
                    .expect("Tx signature length should fit in usize."),
            },
        })
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::LibraryCall(request), vm, syscall_handler)
    }

    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::LibraryCallL1Handler(request), vm, syscall_handler)
    }

    fn replace_class(
        _request: ReplaceClassRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
    ) -> Result<ReplaceClassResponse, Self::Error> {
        Ok(ReplaceClassResponse {})
    }

    fn send_message_to_l1(
        _request: SendMessageToL1Request,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
    ) -> Result<SendMessageToL1Response, Self::Error> {
        Ok(SendMessageToL1Response {})
    }

    fn storage_read(
        _request: StorageReadRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<StorageReadResponse, Self::Error> {
        let value = *syscall_handler.get_mut_call_info_tracker()?.next_execute_code_read()?;
        Ok(StorageReadResponse { value })
    }

    fn storage_write(
        _request: StorageWriteRequest,
        _vm: &mut VirtualMachine,
        _syscall_handler: &mut Self,
    ) -> Result<StorageWriteResponse, Self::Error> {
        Ok(StorageWriteResponse {})
    }
}
