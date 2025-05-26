use blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::{
    DeprecatedSyscallExecutor,
    DeprecatedSyscallExecutorBaseError,
};
use blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallExecutionError;
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
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;

use super::snos_hint_processor::SnosHintProcessor;

#[derive(Debug, thiserror::Error)]
pub enum DeprecatedSnosSyscallError {
    #[error(transparent)]
    SyscallExecutorBase(#[from] DeprecatedSyscallExecutorBaseError),
}

#[allow(unused_variables)]
impl<S: StateReader> DeprecatedSyscallExecutor for SnosHintProcessor<'_, S> {
    type Error = DeprecatedSnosSyscallError;

    fn increment_syscall_count(&mut self, selector: &DeprecatedSyscallSelector) {
        self.deprecated_syscall_hint_processor
            .syscalls_usage
            .entry(*selector)
            .or_default()
            .increment_call_count();
    }

    fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> Result<(), Self::Error> {
        let expected_ptr = self
            .deprecated_syscall_hint_processor
            .syscall_ptr
            .expect("Syscall must be set at this point.");
        if actual_ptr != expected_ptr {
            return Err(DeprecatedSyscallExecutionError::BadSyscallPointer {
                expected_ptr,
                actual_ptr,
            });
        }
        Ok(())
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        self.deprecated_syscall_hint_processor
            .syscall_ptr
            .as_mut()
            .expect("Syscall pointer must be set when executing syscall.")
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<CallContractResponse, Self::Error> {
        todo!()
    }

    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        todo!()
    }

    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        todo!()
    }

    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DeployResponse, Self::Error> {
        todo!()
    }

    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<EmitEventResponse, Self::Error> {
        todo!()
    }

    fn get_block_number(
        request: GetBlockNumberRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockNumberResponse, Self::Error> {
        todo!()
    }

    fn get_block_timestamp(
        request: GetBlockTimestampRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockTimestampResponse, Self::Error> {
        todo!()
    }

    fn get_caller_address(
        request: GetCallerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetCallerAddressResponse, Self::Error> {
        // TODO(Nimrod): Don't unwrap here, use the error handling mechanism.
        let execution_helper = syscall_handler.get_mut_current_execution_helper().unwrap();
        let caller_address = execution_helper
            .tx_execution_iter
            .tx_execution_info_ref
            .as_ref()
            .unwrap()
            .call_info_tracker
            .as_ref()
            .unwrap()
            .call_info
            .call
            .caller_address;
        Ok(GetCallerAddressResponse { address: caller_address })
    }

    fn get_contract_address(
        request: GetContractAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetContractAddressResponse, Self::Error> {
        todo!()
    }

    fn get_sequencer_address(
        request: GetSequencerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetSequencerAddressResponse, Self::Error> {
        todo!()
    }

    fn get_tx_info(
        request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxInfoResponse, Self::Error> {
        todo!()
    }

    fn get_tx_signature(
        request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxSignatureResponse, Self::Error> {
        todo!()
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        todo!()
    }

    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        todo!()
    }

    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<ReplaceClassResponse, Self::Error> {
        todo!()
    }

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<SendMessageToL1Response, Self::Error> {
        todo!()
    }

    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<StorageReadResponse, Self::Error> {
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

    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<StorageWriteResponse, Self::Error> {
        Ok(StorageWriteResponse {})
    }
}
