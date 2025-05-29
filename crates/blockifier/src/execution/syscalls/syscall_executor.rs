use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::execution_resources::GasAmount;
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::{GasCostsError, SyscallGasCost, VersionedConstants};
use crate::execution::execution_utils::felt_from_ptr;
use crate::execution::syscalls::common_syscall_logic::base_keccak;
use crate::execution::syscalls::secp::{
    Secp256r1NewRequest,
    Secp256r1NewResponse,
    SecpAddRequest,
    SecpAddResponse,
    SecpGetPointFromXRequest,
    SecpGetPointFromXResponse,
    SecpGetXyRequest,
    SecpGetXyResponse,
    SecpHintProcessor,
    SecpMulRequest,
    SecpMulResponse,
    SecpNewRequest,
    SecpNewResponse,
};
use crate::execution::syscalls::syscall_base::SyscallResult;
use crate::execution::syscalls::vm_syscall_utils::{
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
    SyscallExecutorBaseError,
    SyscallSelector,
    TryExtractRevert,
};

pub trait SyscallExecutor {
    type Error: From<SyscallExecutorBaseError> + TryExtractRevert;

    #[allow(clippy::result_large_err)]
    fn read_next_syscall_selector(&mut self, vm: &mut VirtualMachine) -> SyscallResult<Felt> {
        Ok(felt_from_ptr(vm, self.get_mut_syscall_ptr())?)
    }

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_keccak_round_cost_base_syscall_cost(&self) -> u64;

    fn get_secpk1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256k1::Config>;

    fn get_secpr1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256r1::Config>;

    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, count: usize);

    fn increment_syscall_count(&mut self, selector: &SyscallSelector) {
        self.increment_syscall_count_by(selector, 1);
    }

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_gas_cost_from_selector(
        &self,
        selector: &SyscallSelector,
    ) -> Result<SyscallGasCost, GasCostsError> {
        self.versioned_constants().os_constants.gas_costs.syscalls.get_syscall_gas_cost(selector)
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable;

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_syscall_base_gas_cost(&self) -> u64 {
        self.versioned_constants().os_constants.gas_costs.base.syscall_base_gas_cost
    }

    fn versioned_constants(&self) -> &VersionedConstants;

    fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount);

    #[allow(clippy::result_large_err)]
    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse>;

    #[allow(clippy::result_large_err)]
    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse>;

    #[allow(clippy::result_large_err)]
    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse>;

    #[allow(clippy::result_large_err)]
    fn get_block_hash(
        request: GetBlockHashRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse>;

    #[allow(clippy::result_large_err)]
    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse>;

    #[allow(clippy::result_large_err)]
    fn get_execution_info(
        request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse>;

    #[allow(clippy::result_large_err)]
    fn keccak(
        request: KeccakRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<KeccakResponse> {
        let input_length = (request.input_end - request.input_start)?;

        let data = vm.get_integer_range(request.input_start, input_length)?;
        let data_u64: &[u64] = &data
            .iter()
            .map(|felt| {
                {
                    felt.to_u64().ok_or_else(|| SyscallExecutorBaseError::InvalidSyscallInput {
                        input: **felt,
                        info: "Invalid input for the keccak syscall.".to_string(),
                    })
                }
            })
            .collect::<Result<Vec<u64>, _>>()?;

        let (state, n_rounds) = base_keccak(
            syscall_handler.get_keccak_round_cost_base_syscall_cost(),
            data_u64,
            remaining_gas,
        )?;

        // For the keccak system call we want to count the number of rounds rather than the number
        // of syscall invocations.
        syscall_handler.increment_syscall_count_by(&SyscallSelector::Keccak, n_rounds);

        Ok(KeccakResponse {
            result_low: (Felt::from(state[1]) * Felt::TWO.pow(64_u128)) + Felt::from(state[0]),
            result_high: (Felt::from(state[3]) * Felt::TWO.pow(64_u128)) + Felt::from(state[2]),
        })
    }

    #[allow(clippy::result_large_err)]
    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse>;

    #[allow(clippy::result_large_err)]
    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response>;

    #[allow(clippy::result_large_err)]
    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse>;

    #[allow(clippy::result_large_err)]
    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse>;

    #[allow(clippy::result_large_err)]
    fn secp256k1_add(
        request: SecpAddRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        Ok(syscall_handler.get_secpk1_hint_processor().secp_add(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256k1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        Ok(syscall_handler.get_secpk1_hint_processor().secp_get_point_from_x(vm, request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256k1_get_xy(
        request: SecpGetXyRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        Ok(syscall_handler.get_secpk1_hint_processor().secp_get_xy(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256k1_mul(
        request: SecpMulRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        Ok(syscall_handler.get_secpk1_hint_processor().secp_mul(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256k1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse> {
        Ok(syscall_handler.get_secpk1_hint_processor().secp_new(vm, request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256r1_add(
        request: SecpAddRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse> {
        Ok(syscall_handler.get_secpr1_hint_processor().secp_add(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256r1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse> {
        Ok(syscall_handler.get_secpr1_hint_processor().secp_get_point_from_x(vm, request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256r1_get_xy(
        request: SecpGetXyRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse> {
        Ok(syscall_handler.get_secpr1_hint_processor().secp_get_xy(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256r1_mul(
        request: SecpMulRequest,
        _vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse> {
        Ok(syscall_handler.get_secpr1_hint_processor().secp_mul(request)?)
    }

    #[allow(clippy::result_large_err)]
    fn secp256r1_new(
        request: Secp256r1NewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        _remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256r1NewResponse> {
        Ok(syscall_handler.get_secpr1_hint_processor().secp_new(vm, request)?)
    }

    #[allow(clippy::result_large_err)]
    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response>;

    #[allow(clippy::result_large_err)]
    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse>;

    #[allow(clippy::result_large_err)]
    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse>;
}
