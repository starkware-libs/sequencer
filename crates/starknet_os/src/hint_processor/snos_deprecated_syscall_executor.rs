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
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::hint_processor::execution_helper::ExecutionHelperError;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::vm_utils::write_to_temp_segment;

#[derive(Debug, thiserror::Error)]
pub enum DeprecatedSnosSyscallError {
    #[error(transparent)]
    ExecutionHelper(#[from] ExecutionHelperError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
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
    ) -> Result<CallContractResponse, DeprecatedSnosSyscallError> {
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

    #[allow(clippy::result_large_err)]
    fn verify_syscall_ptr(&self, actual_ptr: Relocatable) -> Result<(), Self::Error> {
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
    ) -> Result<CallContractResponse, Self::Error> {
        Self::_call_contract(CallRequest::CallContract(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::DelegateCall(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<DelegateCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::DelegateL1Handler(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
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

    #[allow(clippy::result_large_err)]
    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<EmitEventResponse, Self::Error> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_block_number(
        request: GetBlockNumberRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockNumberResponse, Self::Error> {
        let block_number =
            syscall_handler.get_current_execution_helper()?.os_block_input.block_info.block_number;
        Ok(GetBlockNumberResponse { block_number })
    }

    #[allow(clippy::result_large_err)]
    fn get_block_timestamp(
        request: GetBlockTimestampRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetBlockTimestampResponse, Self::Error> {
        let block_timestamp = syscall_handler
            .get_current_execution_helper()?
            .os_block_input
            .block_info
            .block_timestamp;
        Ok(GetBlockTimestampResponse { block_timestamp })
    }

    #[allow(clippy::result_large_err)]
    fn get_caller_address(
        request: GetCallerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetCallerAddressResponse, Self::Error> {
        let caller_address = syscall_handler
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_tx_execution_info_ref()?
            .get_call_info_tracker()?
            .call_info
            .call
            .caller_address;
        Ok(GetCallerAddressResponse { address: caller_address })
    }

    #[allow(clippy::result_large_err)]
    fn get_contract_address(
        request: GetContractAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetContractAddressResponse, Self::Error> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_sequencer_address(
        request: GetSequencerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetSequencerAddressResponse, Self::Error> {
        let sequencer_address = syscall_handler
            .get_current_execution_helper()?
            .os_block_input
            .block_info
            .sequencer_address;
        Ok(GetSequencerAddressResponse { address: sequencer_address })
    }

    #[allow(clippy::result_large_err)]
    fn get_tx_info(
        request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxInfoResponse, Self::Error> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_tx_signature(
        request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<GetTxSignatureResponse, Self::Error> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::LibraryCall(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<LibraryCallResponse, Self::Error> {
        Self::_call_contract(CallRequest::LibraryCallL1Handler(request), vm, syscall_handler)
    }

    #[allow(clippy::result_large_err)]
    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<ReplaceClassResponse, Self::Error> {
        Ok(ReplaceClassResponse {})
    }

    #[allow(clippy::result_large_err)]
    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<SendMessageToL1Response, Self::Error> {
        Ok(SendMessageToL1Response {})
    }

    #[allow(clippy::result_large_err)]
    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<StorageReadResponse, Self::Error> {
        let value = syscall_handler
            .get_mut_current_execution_helper()?
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()?
            .get_mut_call_info_tracker()?
            .next_execute_code_read()?;
        Ok(StorageReadResponse { value })
    }

    #[allow(clippy::result_large_err)]
    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> Result<StorageWriteResponse, Self::Error> {
        Ok(StorageWriteResponse {})
    }
}
