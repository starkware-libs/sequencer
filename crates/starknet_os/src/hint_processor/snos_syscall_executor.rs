use blockifier::blockifier_versioned_constants::{GasCostsError, SyscallGasCost};
use blockifier::execution::syscalls::secp::{
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
use blockifier::execution::syscalls::syscall_base::SyscallResult;
use blockifier::execution::syscalls::syscall_executor::SyscallExecutor;
use blockifier::execution::syscalls::{
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
    SyscallSelector,
};
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::execution_resources::GasAmount;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;

#[allow(unused_variables)]
impl<S: StateReader> SyscallExecutor for SnosHintProcessor<'_, S> {
    fn base_keccak(
        &mut self,
        data: &[u64],
        remaining_gas: &mut u64,
    ) -> SyscallResult<([u64; 4], usize)> {
        todo!()
    }

    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, count: usize) {
        todo!()
    }

    fn get_gas_cost_from_selector(
        &self,
        selector: &SyscallSelector,
    ) -> Result<SyscallGasCost, GasCostsError> {
        todo!()
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        todo!()
    }

    fn get_syscall_base_gas_cost(&self) -> u64 {
        todo!()
    }

    fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount) {
        todo!()
    }

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse> {
        todo!()
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
        todo!()
    }

    fn keccak(
        request: KeccakRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<KeccakResponse> {
        todo!()
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

    fn secp256k1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        todo!()
    }

    fn secp256k1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        todo!()
    }

    fn secp256k1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        todo!()
    }

    fn secp256k1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        todo!()
    }

    fn secp256k1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse> {
        todo!()
    }

    fn secp256r1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        todo!()
    }

    fn secp256r1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        todo!()
    }

    fn secp256r1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        todo!()
    }

    fn secp256r1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        todo!()
    }

    fn secp256r1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse> {
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
        todo!()
    }

    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse> {
        Ok(StorageWriteResponse {})
    }
}
