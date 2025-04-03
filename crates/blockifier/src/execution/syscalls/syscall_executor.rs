use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::vm_core::VirtualMachine;

use super::secp::{
    SecpAddRequest,
    SecpAddResponse,
    SecpGetPointFromXRequest,
    SecpGetPointFromXResponse,
    SecpGetXyRequest,
    SecpGetXyResponse,
    SecpMulRequest,
    SecpMulResponse,
    SecpNewRequest,
    SecpNewResponse,
};
use crate::execution::common_hints::HintExecutionResult;
use crate::execution::syscalls::hint_processor::SyscallHintProcessor;
use crate::execution::syscalls::syscall_base::SyscallResult;
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
    SyscallResponse,
    SyscallSelector,
};

#[allow(dead_code)]
pub trait SyscallExecutor {
    fn execute_syscall_from_selector(
        &mut self,
        vm: &mut VirtualMachine,
        selector: SyscallSelector,
    ) -> HintExecutionResult {
        match selector {
            SyscallSelector::CallContract => {
                self.execute_syscall(vm, selector, Self::call_contract)
            }
            SyscallSelector::Deploy => self.execute_syscall(vm, selector, Self::deploy),
            SyscallSelector::EmitEvent => self.execute_syscall(vm, selector, Self::emit_event),
            SyscallSelector::GetBlockHash => {
                self.execute_syscall(vm, selector, Self::get_block_hash)
            }
            SyscallSelector::GetClassHashAt => {
                self.execute_syscall(vm, selector, Self::get_class_hash_at)
            }
            SyscallSelector::GetExecutionInfo => {
                self.execute_syscall(vm, selector, Self::get_execution_info)
            }
            SyscallSelector::Keccak => self.execute_syscall(vm, selector, Self::keccak),
            SyscallSelector::Sha256ProcessBlock => {
                self.execute_syscall(vm, selector, Self::sha256_process_block)
            }
            SyscallSelector::LibraryCall => self.execute_syscall(vm, selector, Self::library_call),
            SyscallSelector::MetaTxV0 => self.execute_syscall(vm, selector, Self::meta_tx_v0),
            SyscallSelector::ReplaceClass => {
                self.execute_syscall(vm, selector, Self::replace_class)
            }
            SyscallSelector::Secp256k1Add => {
                self.execute_syscall(vm, selector, Self::secp256k1_add)
            }
            SyscallSelector::Secp256k1GetPointFromX => {
                self.execute_syscall(vm, selector, Self::secp256k1_get_point_from_x)
            }
            SyscallSelector::Secp256k1GetXy => {
                self.execute_syscall(vm, selector, Self::secp256k1_get_xy)
            }
            SyscallSelector::Secp256k1Mul => {
                self.execute_syscall(vm, selector, Self::secp256k1_mul)
            }
            SyscallSelector::Secp256k1New => {
                self.execute_syscall(vm, selector, Self::secp256k1_new)
            }
            SyscallSelector::Secp256r1Add => {
                self.execute_syscall(vm, selector, Self::secp256r1_add)
            }
            SyscallSelector::Secp256r1GetPointFromX => {
                self.execute_syscall(vm, selector, Self::secp256r1_get_point_from_x)
            }
            SyscallSelector::Secp256r1GetXy => {
                self.execute_syscall(vm, selector, Self::secp256r1_get_xy)
            }
            SyscallSelector::Secp256r1Mul => {
                self.execute_syscall(vm, selector, Self::secp256r1_mul)
            }
            SyscallSelector::Secp256r1New => {
                self.execute_syscall(vm, selector, Self::secp256r1_new)
            }
            SyscallSelector::SendMessageToL1 => {
                self.execute_syscall(vm, selector, Self::send_message_to_l1)
            }
            SyscallSelector::StorageRead => self.execute_syscall(vm, selector, Self::storage_read),
            SyscallSelector::StorageWrite => {
                self.execute_syscall(vm, selector, Self::storage_write)
            }
            _ => Err(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            )),
        }
    }

    fn execute_syscall<Request, Response, ExecuteCallback>(
        &mut self,
        vm: &mut VirtualMachine,
        selector: SyscallSelector,
        execute_callback: ExecuteCallback,
    ) -> HintExecutionResult
    where
        Request: SyscallRequest + std::fmt::Debug,
        Response: SyscallResponse + std::fmt::Debug,
        ExecuteCallback: FnOnce(
            Request,
            &mut VirtualMachine,
            &mut SyscallHintProcessor<'_>,
            &mut u64, // Remaining gas.
        ) -> SyscallResult<Response>;

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse>;

    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse>;

    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse>;

    fn get_block_hash(
        request: GetBlockHashRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse>;

    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse>;

    fn get_execution_info(
        request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse>;

    fn keccak(
        request: KeccakRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<KeccakResponse>;

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse>;

    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response>;

    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse>;

    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse>;

    fn secp256k1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse>;

    fn secp256k1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse>;
    fn secp256k1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse>;
    fn secp256k1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse>;
    fn secp256k1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse>;
    fn secp256r1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse>;
    fn secp256r1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse>;
    fn secp256r1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse>;
    fn secp256r1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse>;
    fn secp256r1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse>;

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response>;

    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse>;

    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut SyscallHintProcessor<'_>,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse>;
}
