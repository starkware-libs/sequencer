use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;

use crate::execution::common_hints::HintExecutionResult;
use crate::execution::deprecated_syscalls::{
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
    SyscallRequest,
    SyscallResponse,
};

#[allow(dead_code)]
pub trait DeprecatedSyscallExecutor {
    fn execute_syscall_from_selector(
        &mut self,
        vm: &mut VirtualMachine,
        selector: DeprecatedSyscallSelector,
    ) -> HintExecutionResult {
        match selector {
            DeprecatedSyscallSelector::CallContract => {
                self.execute_syscall(vm, Self::call_contract)
            }
            DeprecatedSyscallSelector::DelegateCall => {
                self.execute_syscall(vm, Self::delegate_call)
            }
            DeprecatedSyscallSelector::DelegateL1Handler => {
                self.execute_syscall(vm, Self::delegate_l1_handler)
            }
            DeprecatedSyscallSelector::Deploy => self.execute_syscall(vm, Self::deploy),
            DeprecatedSyscallSelector::EmitEvent => self.execute_syscall(vm, Self::emit_event),
            DeprecatedSyscallSelector::GetBlockNumber => {
                self.execute_syscall(vm, Self::get_block_number)
            }
            DeprecatedSyscallSelector::GetBlockTimestamp => {
                self.execute_syscall(vm, Self::get_block_timestamp)
            }
            DeprecatedSyscallSelector::GetCallerAddress => {
                self.execute_syscall(vm, Self::get_caller_address)
            }
            DeprecatedSyscallSelector::GetContractAddress => {
                self.execute_syscall(vm, Self::get_contract_address)
            }
            DeprecatedSyscallSelector::GetSequencerAddress => {
                self.execute_syscall(vm, Self::get_sequencer_address)
            }
            DeprecatedSyscallSelector::GetTxInfo => self.execute_syscall(vm, Self::get_tx_info),
            DeprecatedSyscallSelector::GetTxSignature => {
                self.execute_syscall(vm, Self::get_tx_signature)
            }
            DeprecatedSyscallSelector::LibraryCall => self.execute_syscall(vm, Self::library_call),
            DeprecatedSyscallSelector::LibraryCallL1Handler => {
                self.execute_syscall(vm, Self::library_call_l1_handler)
            }
            DeprecatedSyscallSelector::ReplaceClass => {
                self.execute_syscall(vm, Self::replace_class)
            }
            DeprecatedSyscallSelector::SendMessageToL1 => {
                self.execute_syscall(vm, Self::send_message_to_l1)
            }
            DeprecatedSyscallSelector::StorageRead => self.execute_syscall(vm, Self::storage_read),
            DeprecatedSyscallSelector::StorageWrite => {
                self.execute_syscall(vm, Self::storage_write)
            }
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            )),
        }
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable;

    fn execute_syscall<Request, Response, ExecuteCallback>(
        &mut self,
        vm: &mut VirtualMachine,
        execute_callback: ExecuteCallback,
    ) -> HintExecutionResult
    where
        Request: SyscallRequest,
        Response: SyscallResponse,
        ExecuteCallback:
            FnOnce(Request, &mut VirtualMachine, &mut Self) -> DeprecatedSyscallResult<Response>,
    {
        let request = Request::read(vm, self.get_mut_syscall_ptr())?;

        let response = execute_callback(request, vm, self)?;
        response.write(vm, self.get_mut_syscall_ptr())?;

        Ok(())
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<CallContractResponse>;
    fn delegate_call(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse>;
    fn delegate_l1_handler(
        request: DelegateCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DelegateCallResponse>;
    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<DeployResponse>;
    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<EmitEventResponse>;
    fn get_block_number(
        request: GetBlockNumberRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockNumberResponse>;
    fn get_block_timestamp(
        request: GetBlockTimestampRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetBlockTimestampResponse>;
    fn get_caller_address(
        request: GetCallerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetCallerAddressResponse>;
    fn get_contract_address(
        request: GetContractAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetContractAddressResponse>;
    fn get_sequencer_address(
        request: GetSequencerAddressRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetSequencerAddressResponse>;
    fn get_tx_info(
        request: GetTxInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxInfoResponse>;
    fn get_tx_signature(
        request: GetTxSignatureRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<GetTxSignatureResponse>;
    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse>;
    fn library_call_l1_handler(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<LibraryCallResponse>;
    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<ReplaceClassResponse>;
    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<SendMessageToL1Response>;
    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageReadResponse>;
    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
    ) -> DeprecatedSyscallResult<StorageWriteResponse>;
}
