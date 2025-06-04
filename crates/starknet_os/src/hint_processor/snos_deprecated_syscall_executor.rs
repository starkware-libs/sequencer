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
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::vm_utils::write_to_temp_segment;

#[derive(Debug, thiserror::Error)]
pub enum DeprecatedSnosSyscallError {
    #[error(transparent)]
    SyscallExecutorBase(#[from] DeprecatedSyscallExecutorBaseError),
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
    #[allow(clippy::result_large_err)]
    fn _call_contract(
        request: CallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<CallContractResponse> {
        let next_call_execution = syscall_handler.get_next_call_execution();

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

    #[allow(clippy::result_large_err)]
    fn get_call_entry_point(&mut self) -> DeprecatedSyscallResult<&CallEntryPoint> {
        Ok(&self
            .get_mut_current_execution_helper()
            .expect("Execution helper must be set when executing syscall.")
            .tx_execution_iter
            .tx_execution_info_ref
            .as_mut()
            .expect("Tx execution info must be set when executing syscall.")
            .call_info_tracker
            .as_mut()
            .expect("Call info tracker must be set when executing syscall.")
            .call_info
            .call)
    }
}

#[allow(unused_variables)]
impl<S: StateReader> DeprecatedSyscallExecutor for SnosHintProcessor<'_, S> {
    fn increment_syscall_count(&mut self, selector: &DeprecatedSyscallSelector) {
        self.deprecated_syscall_hint_processor
            .syscalls_usage
            .entry(*selector)
            .or_default()
            .increment_call_count();
    }

    #[allow(clippy::result_large_err)]
    fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> DeprecatedSyscallResult<()> {
        let expected_ptr = self
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
        self.deprecated_syscall_hint_processor
            .syscall_ptr
            .as_mut()
            .expect("Syscall pointer must be set when executing syscall.")
    }

    #[allow(clippy::result_large_err)]
    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<CallContractResponse> {
        Self::_call_contract(CallRequest::CallContract(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse> {
        Self::_call_contract(CallRequest::DelegateCall(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse> {
        Self::_call_contract(CallRequest::DelegateL1Handler(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DeployResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<EmitEventResponse> {
        Ok(EmitEventResponse {})
    }

    #[allow(clippy::result_large_err)]
    fn get_block_number(
        request: GetBlockNumberRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockNumberResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_block_timestamp(
        request: GetBlockTimestampRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockTimestampResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_caller_address(
        request: GetCallerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetCallerAddressResponse> {
        Ok(GetCallerAddressResponse {
            address: syscall_handler.get_call_entry_point()?.caller_address,
        })
    }

    #[allow(clippy::result_large_err)]
    fn get_contract_address(
        request: GetContractAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetContractAddressResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_sequencer_address(
        request: GetSequencerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetSequencerAddressResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_tx_info(
        request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxInfoResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_tx_signature(
        request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxSignatureResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse> {
        Self::_call_contract(CallRequest::LibraryCall(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse> {
        Self::_call_contract(CallRequest::LibraryCallL1Handler(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<ReplaceClassResponse> {
        Ok(ReplaceClassResponse {})
    }

    #[allow(clippy::result_large_err)]
    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<SendMessageToL1Response> {
        Ok(SendMessageToL1Response {})
    }

    #[allow(clippy::result_large_err)]
    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageReadResponse> {
        // TODO(Nimrod): Don't unwrap here, use the error handling mechanism.
        let execution_helper = syscall_handler.get_mut_current_execution_helper().unwrap();
        let value = *execution_helper
            .tx_execution_iter
            .tx_execution_info_ref
            .as_mut()
            .unwrap()
            .call_info_tracker
            .as_mut()
            .unwrap()
            .execute_code_read_iterator
            .next()
            .unwrap();
        Ok(StorageReadResponse { value })
    }

    #[allow(clippy::result_large_err)]
    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageWriteResponse> {
        Ok(StorageWriteResponse {})
    }
}
