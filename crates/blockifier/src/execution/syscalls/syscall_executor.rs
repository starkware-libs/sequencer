use cairo_lang_casm::hints::StarknetHint;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::execution_resources::GasAmount;
use starknet_types_core::felt::Felt;

use super::hint_processor::INVALID_INPUT_LENGTH_ERROR;
use super::syscall_base::KECCAK_FULL_RATE_IN_WORDS;
use crate::blockifier_versioned_constants::{GasCostsError, SyscallGasCost};
use crate::execution::common_hints::HintExecutionResult;
use crate::execution::execution_utils::felt_from_ptr;
use crate::execution::syscalls::hint_processor::{SyscallExecutionError, OUT_OF_GAS_ERROR};
use crate::execution::syscalls::secp::{
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
    SyscallRequestWrapper,
    SyscallResponse,
    SyscallResponseWrapper,
    SyscallSelector,
};
use crate::utils::u64_from_usize;

pub trait SyscallExecutor {
    fn read_next_syscall_selector(&mut self, vm: &mut VirtualMachine) -> SyscallResult<Felt> {
        Ok(felt_from_ptr(vm, self.get_mut_syscall_ptr())?)
    }

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_keccak_round_cost_base_syscall_cost(&self) -> u64;

    fn base_keccak(
        &mut self,
        input: &[u64],
        remaining_gas: &mut u64,
    ) -> SyscallResult<([u64; 4], usize)> {
        let input_length = input.len();

        let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

        if remainder != 0 {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![
                    Felt::from_hex(INVALID_INPUT_LENGTH_ERROR)
                        .expect("Failed to parse INVALID_INPUT_LENGTH_ERROR hex string"),
                ],
            });
        }
        // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
        // works.
        let n_rounds_as_u64 = u64::try_from(n_rounds).expect("Failed to convert usize to u64.");
        let gas_cost = n_rounds_as_u64 * self.get_keccak_round_cost_base_syscall_cost();

        if gas_cost > *remaining_gas {
            let out_of_gas_error = Felt::from_hex(OUT_OF_GAS_ERROR)
                .expect("Failed to parse OUT_OF_GAS_ERROR hex string");

            return Err(SyscallExecutionError::Revert { error_data: vec![out_of_gas_error] });
        }
        *remaining_gas -= gas_cost;

        let mut state = [0u64; 25];
        for chunk in input.chunks(KECCAK_FULL_RATE_IN_WORDS) {
            for (i, val) in chunk.iter().enumerate() {
                state[i] ^= val;
            }
            keccak::f1600(&mut state)
        }

        Ok((state[..4].try_into().expect("Slice with incorrect length"), n_rounds))
    }

    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, count: usize);

    fn increment_syscall_count(&mut self, selector: &SyscallSelector) {
        self.increment_syscall_count_by(selector, 1);
    }

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_gas_cost_from_selector(
        &self,
        selector: &SyscallSelector,
    ) -> Result<SyscallGasCost, GasCostsError>;

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable;

    // TODO(Aner): replace function with inline after implementing fn get_gas_costs.
    fn get_syscall_base_gas_cost(&self) -> u64;

    fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount);

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse>;

    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse>;

    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse>;

    fn get_block_hash(
        request: GetBlockHashRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse>;

    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse>;

    fn get_execution_info(
        request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse>;

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
                felt.to_u64().ok_or_else(|| SyscallExecutionError::InvalidSyscallInput {
                    input: **felt,
                    info: "Invalid input for the keccak syscall.".to_string(),
                })
            })
            .collect::<Result<Vec<u64>, _>>()?;

        let (state, n_rounds) = syscall_handler.base_keccak(data_u64, remaining_gas)?;

        // For the keccak system call we want to count the number of rounds rather than the number
        // of syscall invocations.
        syscall_handler.increment_syscall_count_by(&SyscallSelector::Keccak, n_rounds);

        Ok(KeccakResponse {
            result_low: (Felt::from(state[1]) * Felt::TWO.pow(64_u128)) + Felt::from(state[0]),
            result_high: (Felt::from(state[3]) * Felt::TWO.pow(64_u128)) + Felt::from(state[2]),
        })
    }

    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse>;

    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response>;

    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse>;

    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse>;

    fn secp256k1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse>;

    fn secp256k1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse>;

    fn secp256k1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse>;

    fn secp256k1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse>;

    fn secp256k1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse>;

    fn secp256r1_add(
        request: SecpAddRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpAddResponse>;

    fn secp256r1_get_point_from_x(
        request: SecpGetPointFromXRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetPointFromXResponse>;

    fn secp256r1_get_xy(
        request: SecpGetXyRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpGetXyResponse>;

    fn secp256r1_mul(
        request: SecpMulRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpMulResponse>;

    fn secp256r1_new(
        request: SecpNewRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SecpNewResponse>;

    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response>;

    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse>;

    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse>;
}

pub(crate) fn execute_syscall_from_selector<T: SyscallExecutor>(
    syscall_executor: &mut T,
    vm: &mut VirtualMachine,
    selector: SyscallSelector,
) -> HintExecutionResult {
    match selector {
        SyscallSelector::CallContract => {
            execute_syscall(syscall_executor, vm, selector, T::call_contract)
        }
        SyscallSelector::Deploy => execute_syscall(syscall_executor, vm, selector, T::deploy),
        SyscallSelector::EmitEvent => {
            execute_syscall(syscall_executor, vm, selector, T::emit_event)
        }
        SyscallSelector::GetBlockHash => {
            execute_syscall(syscall_executor, vm, selector, T::get_block_hash)
        }
        SyscallSelector::GetClassHashAt => {
            execute_syscall(syscall_executor, vm, selector, T::get_class_hash_at)
        }
        SyscallSelector::GetExecutionInfo => {
            execute_syscall(syscall_executor, vm, selector, T::get_execution_info)
        }
        SyscallSelector::Keccak => execute_syscall(syscall_executor, vm, selector, T::keccak),
        SyscallSelector::Sha256ProcessBlock => {
            execute_syscall(syscall_executor, vm, selector, T::sha256_process_block)
        }
        SyscallSelector::LibraryCall => {
            execute_syscall(syscall_executor, vm, selector, T::library_call)
        }
        SyscallSelector::MetaTxV0 => execute_syscall(syscall_executor, vm, selector, T::meta_tx_v0),
        SyscallSelector::ReplaceClass => {
            execute_syscall(syscall_executor, vm, selector, T::replace_class)
        }
        SyscallSelector::Secp256k1Add => {
            execute_syscall(syscall_executor, vm, selector, T::secp256k1_add)
        }
        SyscallSelector::Secp256k1GetPointFromX => {
            execute_syscall(syscall_executor, vm, selector, T::secp256k1_get_point_from_x)
        }
        SyscallSelector::Secp256k1GetXy => {
            execute_syscall(syscall_executor, vm, selector, T::secp256k1_get_xy)
        }
        SyscallSelector::Secp256k1Mul => {
            execute_syscall(syscall_executor, vm, selector, T::secp256k1_mul)
        }
        SyscallSelector::Secp256k1New => {
            execute_syscall(syscall_executor, vm, selector, T::secp256k1_new)
        }
        SyscallSelector::Secp256r1Add => {
            execute_syscall(syscall_executor, vm, selector, T::secp256r1_add)
        }
        SyscallSelector::Secp256r1GetPointFromX => {
            execute_syscall(syscall_executor, vm, selector, T::secp256r1_get_point_from_x)
        }
        SyscallSelector::Secp256r1GetXy => {
            execute_syscall(syscall_executor, vm, selector, T::secp256r1_get_xy)
        }
        SyscallSelector::Secp256r1Mul => {
            execute_syscall(syscall_executor, vm, selector, T::secp256r1_mul)
        }
        SyscallSelector::Secp256r1New => {
            execute_syscall(syscall_executor, vm, selector, T::secp256r1_new)
        }
        SyscallSelector::SendMessageToL1 => {
            execute_syscall(syscall_executor, vm, selector, T::send_message_to_l1)
        }
        SyscallSelector::StorageRead => {
            execute_syscall(syscall_executor, vm, selector, T::storage_read)
        }
        SyscallSelector::StorageWrite => {
            execute_syscall(syscall_executor, vm, selector, T::storage_write)
        }
        // Explicitly list unsupported syscalls, so compiler can catch if a syscall is missing.
        SyscallSelector::DelegateCall
        | SyscallSelector::DelegateL1Handler
        | SyscallSelector::GetBlockNumber
        | SyscallSelector::GetBlockTimestamp
        | SyscallSelector::GetCallerAddress
        | SyscallSelector::GetContractAddress
        | SyscallSelector::GetSequencerAddress
        | SyscallSelector::GetTxInfo
        | SyscallSelector::GetTxSignature
        | SyscallSelector::KeccakRound
        | SyscallSelector::LibraryCallL1Handler => Err(HintError::UnknownHint(
            format!("Unsupported syscall selector {selector:?}.").into(),
        )),
    }
}

fn execute_syscall<Request, Response, ExecuteCallback, Executor>(
    syscall_executor: &mut Executor,
    vm: &mut VirtualMachine,
    selector: SyscallSelector,
    execute_callback: ExecuteCallback,
) -> HintExecutionResult
where
    Executor: SyscallExecutor,
    Request: SyscallRequest + std::fmt::Debug,
    Response: SyscallResponse + std::fmt::Debug,
    ExecuteCallback: FnOnce(
        Request,
        &mut VirtualMachine,
        &mut Executor,
        &mut u64, // Remaining gas.
    ) -> SyscallResult<Response>,
{
    let syscall_gas_cost = syscall_executor.get_gas_cost_from_selector(&selector).map_err(|e| {
        HintError::CustomHint(
            format!("Failed to get gas cost for syscall selector {selector:?}. Error: {e:?}")
                .into(),
        )
    })?;

    let SyscallRequestWrapper { gas_counter, request } =
        SyscallRequestWrapper::<Request>::read(vm, syscall_executor.get_mut_syscall_ptr())?;

    let syscall_gas_cost =
        syscall_gas_cost.get_syscall_cost(u64_from_usize(request.get_linear_factor_length()));
    let syscall_base_cost = syscall_executor.get_syscall_base_gas_cost();

    // Sanity check for preventing underflow.
    assert!(
        syscall_gas_cost >= syscall_base_cost,
        "Syscall gas cost must be greater than base syscall gas cost"
    );

    // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
    let required_gas = syscall_gas_cost - syscall_base_cost;

    if gas_counter < required_gas {
        //  Out of gas failure.
        let out_of_gas_error =
            Felt::from_hex(OUT_OF_GAS_ERROR).map_err(SyscallExecutionError::from)?;
        let response: SyscallResponseWrapper<Response> =
            SyscallResponseWrapper::Failure { gas_counter, error_data: vec![out_of_gas_error] };
        response.write(vm, syscall_executor.get_mut_syscall_ptr())?;

        return Ok(());
    }

    // Execute.
    let mut remaining_gas = gas_counter - required_gas;

    // To support sierra gas charge for blockifier revert flow, we track the remaining gas left
    // before executing a syscall if the current tracked resource is gas.
    // 1. If the syscall does not run Cairo code (i.e. not library call, not call contract, and not
    //    a deploy), any failure will not run in the OS, so no need to charge - the value before
    //    entering the callback is good enough to charge.
    // 2. If the syscall runs Cairo code, but the tracked resource is steps (and not gas), the
    //    additional charge of reverted cairo steps will cover the inner cost, and the outer cost we
    //    track here will be the additional reverted gas.
    // 3. If the syscall runs Cairo code and the tracked resource is gas, either the inner failure
    //    will be a Cairo1 revert (and the gas consumed on the call info will override the current
    //    tracked value), or we will pass through another syscall before failing - and by induction
    //    (we will reach this point again), the gas will be charged correctly.
    syscall_executor.update_revert_gas_with_next_remaining_gas(GasAmount(remaining_gas));

    let original_response = execute_callback(request, vm, syscall_executor, &mut remaining_gas);
    let response = match original_response {
        Ok(response) => SyscallResponseWrapper::Success { gas_counter: remaining_gas, response },
        Err(SyscallExecutionError::Revert { error_data: data }) => {
            SyscallResponseWrapper::Failure { gas_counter: remaining_gas, error_data: data }
        }
        Err(error) => return Err(error.into()),
    };

    response.write(vm, syscall_executor.get_mut_syscall_ptr())?;

    Ok(())
}

/// Infers and executes the next syscall.
/// Must comply with the API of a hint function, as defined by the `HintProcessor`.
pub fn execute_next_syscall<T: SyscallExecutor>(
    syscall_executor: &mut T,
    vm: &mut VirtualMachine,
    hint: &StarknetHint,
) -> HintExecutionResult {
    let StarknetHint::SystemCall { .. } = hint else {
        return Err(HintError::Internal(VirtualMachineError::Other(anyhow::anyhow!(
            "Test functions are unsupported on starknet."
        ))));
    };

    let selector = SyscallSelector::try_from(syscall_executor.read_next_syscall_selector(vm)?)?;

    // Keccak resource usage depends on the input length, so we increment the syscall count
    // in the syscall execution callback.
    if selector != SyscallSelector::Keccak {
        syscall_executor.increment_syscall_count(&selector);
    }

    execute_syscall_from_selector(syscall_executor, vm, selector)
}
