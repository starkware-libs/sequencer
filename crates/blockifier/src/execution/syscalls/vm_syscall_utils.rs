use std::collections::HashMap;

use cairo_lang_casm::hints::StarknetHint;
use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, TransactionSignature};
use starknet_api::transaction::{EventContent, EventData, EventKey, L2ToL1Payload};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::{Felt, FromStrError};

use crate::abi::sierra_types::SierraTypeError;
use crate::blockifier_versioned_constants::{EventLimits, GasCostsError, VersionedConstants};
use crate::execution::call_info::MessageToL1;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::deprecated_syscalls::deprecated_syscall_executor::DeprecatedSyscallExecutorBaseError;
use crate::execution::deprecated_syscalls::DeprecatedSyscallSelector;
use crate::execution::execution_utils::{
    felt_from_ptr,
    write_felt,
    write_maybe_relocatable,
    ReadOnlySegment,
};
use crate::execution::syscalls::hint_processor::{
    felt_to_bool,
    read_call_params,
    read_calldata,
    read_felt_array,
    write_segment,
    EmitEventError,
    OUT_OF_GAS_ERROR,
};
use crate::execution::syscalls::syscall_executor::SyscallExecutor;
use crate::utils::u64_from_usize;

pub type WriteResponseResult = SyscallBaseResult<()>;

pub type SyscallSelector = DeprecatedSyscallSelector;

pub type SyscallUsageMap = HashMap<SyscallSelector, SyscallUsage>;

#[derive(Clone, Debug, Default)]
pub struct SyscallUsage {
    pub call_count: usize,
    pub linear_factor: usize,
}

impl SyscallUsage {
    pub fn new(call_count: usize, linear_factor: usize) -> Self {
        SyscallUsage { call_count, linear_factor }
    }

    pub fn increment_call_count(&mut self) {
        self.call_count += 1;
    }
}

pub trait SyscallRequest: Sized {
    fn read(_vm: &VirtualMachine, _ptr: &mut Relocatable) -> SyscallBaseResult<Self>;

    /// Returns the linear factor's length for the syscall.
    /// If no factor exists, it returns 0.
    fn get_linear_factor_length(&self) -> usize {
        0
    }
}

pub trait SyscallResponse {
    fn write(self, _vm: &mut VirtualMachine, _ptr: &mut Relocatable) -> WriteResponseResult;
}

// Syscall header structs.
pub struct SyscallRequestWrapper<T: SyscallRequest> {
    pub gas_counter: u64,
    pub request: T,
}
impl<T: SyscallRequest> SyscallRequest for SyscallRequestWrapper<T> {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<Self> {
        let gas_counter = felt_from_ptr(vm, ptr)?;
        let gas_counter =
            gas_counter.to_u64().ok_or_else(|| SyscallExecutorBaseError::InvalidSyscallInput {
                input: gas_counter,
                info: String::from("Unexpected gas."),
            })?;
        Ok(Self { gas_counter, request: T::read(vm, ptr)? })
    }
}

pub enum SyscallResponseWrapper<T: SyscallResponse> {
    Success { gas_counter: u64, response: T },
    Failure { gas_counter: u64, revert_data: RevertData },
}
impl<T: SyscallResponse> SyscallResponse for SyscallResponseWrapper<T> {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        match self {
            Self::Success { gas_counter, response } => {
                write_felt(vm, ptr, Felt::from(gas_counter))?;
                // 0 to indicate success.
                write_felt(vm, ptr, Felt::ZERO)?;
                response.write(vm, ptr)
            }
            Self::Failure {
                gas_counter,
                revert_data: RevertData { error_data, use_temp_segment },
            } => {
                write_felt(vm, ptr, Felt::from(gas_counter))?;
                // 1 to indicate failure.
                write_felt(vm, ptr, Felt::ONE)?;

                // Write the error data to a new memory segment depends on the segment type.
                let revert_reason_start = match use_temp_segment {
                    true => vm.add_temporary_segment(),
                    false => vm.add_memory_segment(),
                };
                let revert_reason_end = vm.load_data(
                    revert_reason_start,
                    &error_data.into_iter().map(Into::into).collect::<Vec<MaybeRelocatable>>(),
                )?;

                // Write the start and end pointers of the error data.
                write_maybe_relocatable(vm, ptr, revert_reason_start)?;
                write_maybe_relocatable(vm, ptr, revert_reason_end)?;
                Ok(())
            }
        }
    }
}

// Common structs.

#[derive(Debug, Eq, PartialEq)]
pub struct EmptyRequest;

impl SyscallRequest for EmptyRequest {
    fn read(_vm: &VirtualMachine, _ptr: &mut Relocatable) -> SyscallBaseResult<EmptyRequest> {
        Ok(EmptyRequest)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct EmptyResponse;

impl SyscallResponse for EmptyResponse {
    fn write(self, _vm: &mut VirtualMachine, _ptr: &mut Relocatable) -> WriteResponseResult {
        Ok(())
    }
}

#[derive(Debug)]
pub struct SingleSegmentResponse {
    pub segment: ReadOnlySegment,
}

impl SyscallResponse for SingleSegmentResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_segment(vm, ptr, self.segment)
    }
}

// CallContract syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct CallContractRequest {
    pub contract_address: ContractAddress,
    pub function_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl SyscallRequest for CallContractRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<CallContractRequest> {
        let contract_address = ContractAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let (function_selector, calldata) = read_call_params(vm, ptr)?;

        Ok(CallContractRequest { contract_address, function_selector, calldata })
    }
}

pub type CallContractResponse = SingleSegmentResponse;

// Deploy syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct DeployRequest {
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub deploy_from_zero: bool,
}

impl SyscallRequest for DeployRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<DeployRequest> {
        let class_hash = ClassHash(felt_from_ptr(vm, ptr)?);
        let contract_address_salt = ContractAddressSalt(felt_from_ptr(vm, ptr)?);
        let constructor_calldata = read_calldata(vm, ptr)?;
        let deploy_from_zero = felt_from_ptr(vm, ptr)?;

        Ok(DeployRequest {
            class_hash,
            contract_address_salt,
            constructor_calldata,
            deploy_from_zero: felt_to_bool(
                deploy_from_zero,
                "The deploy_from_zero field in the deploy system call must be 0 or 1.",
            )?,
        })
    }

    fn get_linear_factor_length(&self) -> usize {
        self.constructor_calldata.0.len()
    }
}

#[derive(Debug)]
pub struct DeployResponse {
    pub contract_address: ContractAddress,
    pub constructor_retdata: ReadOnlySegment,
}

impl SyscallResponse for DeployResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, *self.contract_address.0.key())?;
        write_segment(vm, ptr, self.constructor_retdata)
    }
}

// EmitEvent syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct EmitEventRequest {
    pub content: EventContent,
}

impl SyscallRequest for EmitEventRequest {
    // The Cairo struct contains: `keys_len`, `keys`, `data_len`, `data`·
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<EmitEventRequest> {
        let keys = read_felt_array::<SyscallExecutorBaseError>(vm, ptr)?
            .into_iter()
            .map(EventKey)
            .collect();
        let data = EventData(read_felt_array::<SyscallExecutorBaseError>(vm, ptr)?);

        Ok(EmitEventRequest { content: EventContent { keys, data } })
    }
}

pub type EmitEventResponse = EmptyResponse;

pub fn exceeds_event_size_limit(
    versioned_constants: &VersionedConstants,
    n_emitted_events: usize,
    event: &EventContent,
) -> Result<(), EmitEventError> {
    let EventLimits { max_data_length, max_keys_length, max_n_emitted_events } =
        versioned_constants.tx_event_limits;
    if n_emitted_events > max_n_emitted_events {
        return Err(EmitEventError::ExceedsMaxNumberOfEmittedEvents {
            n_emitted_events,
            max_n_emitted_events,
        });
    }
    let keys_length = event.keys.len();
    if keys_length > max_keys_length {
        return Err(EmitEventError::ExceedsMaxKeysLength { keys_length, max_keys_length });
    }
    let data_length = event.data.0.len();
    if data_length > max_data_length {
        return Err(EmitEventError::ExceedsMaxDataLength { data_length, max_data_length });
    }

    Ok(())
}

// GetBlockHash syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct GetBlockHashRequest {
    pub block_number: BlockNumber,
}

impl SyscallRequest for GetBlockHashRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<GetBlockHashRequest> {
        let felt = felt_from_ptr(vm, ptr)?;
        let block_number = BlockNumber(felt.to_u64().ok_or_else(|| {
            SyscallExecutorBaseError::InvalidSyscallInput {
                input: felt,
                info: String::from("Block number must fit within 64 bits."),
            }
        })?);

        Ok(GetBlockHashRequest { block_number })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct GetBlockHashResponse {
    pub block_hash: BlockHash,
}

impl SyscallResponse for GetBlockHashResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, self.block_hash.0)?;
        Ok(())
    }
}

// GetExecutionInfo syscall.

pub type GetExecutionInfoRequest = EmptyRequest;

#[derive(Debug, Eq, PartialEq)]
pub struct GetExecutionInfoResponse {
    pub execution_info_ptr: Relocatable,
}

impl SyscallResponse for GetExecutionInfoResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, self.execution_info_ptr)?;
        Ok(())
    }
}

// LibraryCall syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct LibraryCallRequest {
    pub class_hash: ClassHash,
    pub function_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl SyscallRequest for LibraryCallRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<LibraryCallRequest> {
        let class_hash = ClassHash(felt_from_ptr(vm, ptr)?);
        let (function_selector, calldata) = read_call_params(vm, ptr)?;

        Ok(LibraryCallRequest { class_hash, function_selector, calldata })
    }
}

pub type LibraryCallResponse = CallContractResponse;

// MetaTxV0 syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct MetaTxV0Request {
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
}

impl SyscallRequest for MetaTxV0Request {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<MetaTxV0Request> {
        let contract_address = ContractAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let (entry_point_selector, calldata) = read_call_params(vm, ptr)?;
        let signature =
            TransactionSignature(read_felt_array::<SyscallExecutorBaseError>(vm, ptr)?.into());

        Ok(MetaTxV0Request { contract_address, entry_point_selector, calldata, signature })
    }

    fn get_linear_factor_length(&self) -> usize {
        self.calldata.0.len()
    }
}

pub type MetaTxV0Response = CallContractResponse;

// ReplaceClass syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct ReplaceClassRequest {
    pub class_hash: ClassHash,
}

impl SyscallRequest for ReplaceClassRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<ReplaceClassRequest> {
        let class_hash = ClassHash(felt_from_ptr(vm, ptr)?);

        Ok(ReplaceClassRequest { class_hash })
    }
}

pub type ReplaceClassResponse = EmptyResponse;

// SendMessageToL1 syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct SendMessageToL1Request {
    pub message: MessageToL1,
}

impl SyscallRequest for SendMessageToL1Request {
    // The Cairo struct contains: `to_address`, `payload_size`, `payload`.
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> SyscallBaseResult<SendMessageToL1Request> {
        let to_address = EthAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let payload = L2ToL1Payload(read_felt_array::<SyscallExecutorBaseError>(vm, ptr)?);

        Ok(SendMessageToL1Request { message: MessageToL1 { to_address, payload } })
    }
}

pub type SendMessageToL1Response = EmptyResponse;

// TODO(spapini): Do something with address domain in read and write.
// StorageRead syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct StorageReadRequest {
    pub address_domain: Felt,
    pub address: StorageKey,
}

impl SyscallRequest for StorageReadRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<StorageReadRequest> {
        let address_domain = felt_from_ptr(vm, ptr)?;
        if address_domain != Felt::ZERO {
            return Err(SyscallExecutorBaseError::InvalidAddressDomain { address_domain });
        }
        let address = StorageKey::try_from(felt_from_ptr(vm, ptr)?)?;
        Ok(StorageReadRequest { address_domain, address })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct StorageReadResponse {
    pub value: Felt,
}

impl SyscallResponse for StorageReadResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, self.value)?;
        Ok(())
    }
}

// StorageWrite syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct StorageWriteRequest {
    pub address_domain: Felt,
    pub address: StorageKey,
    pub value: Felt,
}

impl SyscallRequest for StorageWriteRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<StorageWriteRequest> {
        let address_domain = felt_from_ptr(vm, ptr)?;
        if address_domain != Felt::ZERO {
            return Err(SyscallExecutorBaseError::InvalidAddressDomain { address_domain });
        }
        let address = StorageKey::try_from(felt_from_ptr(vm, ptr)?)?;
        let value = felt_from_ptr(vm, ptr)?;
        Ok(StorageWriteRequest { address_domain, address, value })
    }
}

pub type StorageWriteResponse = EmptyResponse;

// Keccak syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct KeccakRequest {
    pub input_start: Relocatable,
    pub input_end: Relocatable,
}

impl SyscallRequest for KeccakRequest {
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallBaseResult<KeccakRequest> {
        let input_start = vm.get_relocatable(*ptr)?;
        *ptr = (*ptr + 1)?;
        let input_end = vm.get_relocatable(*ptr)?;
        *ptr = (*ptr + 1)?;
        Ok(KeccakRequest { input_start, input_end })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct KeccakResponse {
    pub result_low: Felt,
    pub result_high: Felt,
}

impl SyscallResponse for KeccakResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, self.result_low)?;
        write_felt(vm, ptr, self.result_high)?;
        Ok(())
    }
}

// Sha256ProcessBlock syscall.
#[derive(Debug, Eq, PartialEq)]
pub struct Sha256ProcessBlockRequest {
    pub state_ptr: Relocatable,
    pub input_start: Relocatable,
}

impl SyscallRequest for Sha256ProcessBlockRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> SyscallBaseResult<Sha256ProcessBlockRequest> {
        let state_start = vm.get_relocatable(*ptr)?;
        *ptr = (*ptr + 1)?;
        let input_start = vm.get_relocatable(*ptr)?;
        *ptr = (*ptr + 1)?;
        Ok(Sha256ProcessBlockRequest { state_ptr: state_start, input_start })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Sha256ProcessBlockResponse {
    pub state_ptr: Relocatable,
}

impl SyscallResponse for Sha256ProcessBlockResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, self.state_ptr)?;
        Ok(())
    }
}

// GetClassHashAt syscall.

pub type GetClassHashAtRequest = ContractAddress;
pub type GetClassHashAtResponse = ClassHash;

impl SyscallRequest for GetClassHashAtRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> SyscallBaseResult<GetClassHashAtRequest> {
        let address = ContractAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        Ok(address)
    }
}

impl SyscallResponse for GetClassHashAtResponse {
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, *self)?;
        Ok(())
    }
}
// Execution.

pub(crate) fn execute_syscall_from_selector<T: SyscallExecutor>(
    syscall_executor: &mut T,
    vm: &mut VirtualMachine,
    selector: SyscallSelector,
) -> Result<(), T::Error> {
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
        | SyscallSelector::LibraryCallL1Handler => {
            Err(T::Error::from(SyscallExecutorBaseError::from(HintError::UnknownHint(
                format!("Unsupported syscall selector {selector:?}.").into(),
            ))))
        }
    }
}

fn execute_syscall<Request, Response, ExecuteCallback, Executor>(
    syscall_executor: &mut Executor,
    vm: &mut VirtualMachine,
    selector: SyscallSelector,
    execute_callback: ExecuteCallback,
) -> Result<(), Executor::Error>
where
    Executor: SyscallExecutor,
    Request: SyscallRequest + std::fmt::Debug,
    Response: SyscallResponse + std::fmt::Debug,
    ExecuteCallback: FnOnce(
        Request,
        &mut VirtualMachine,
        &mut Executor,
        &mut u64, // Remaining gas.
    ) -> Result<Response, Executor::Error>,
{
    let syscall_gas_cost = syscall_executor
        .get_gas_cost_from_selector(&selector)
        .map_err(|error| SyscallExecutorBaseError::GasCost { error, selector })?;

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
            Felt::from_hex(OUT_OF_GAS_ERROR).map_err(SyscallExecutorBaseError::from)?;
        let response: SyscallResponseWrapper<Response> = SyscallResponseWrapper::Failure {
            gas_counter,
            revert_data: RevertData::new_normal(vec![out_of_gas_error]),
        };
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
        Err(error) => match error.try_extract_revert() {
            SelfOrRevert::Revert(data) => {
                SyscallResponseWrapper::Failure { gas_counter: remaining_gas, revert_data: data }
            }
            SelfOrRevert::Original(err) => return Err(err),
        },
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
) -> Result<(), T::Error> {
    let StarknetHint::SystemCall { .. } = hint else {
        return Err(SyscallExecutorBaseError::from(VirtualMachineError::Other(anyhow::anyhow!(
            "Test functions are unsupported on starknet."
        ))))?;
    };

    let selector = SyscallSelector::try_from(syscall_executor.read_next_syscall_selector(vm)?)
        .map_err(SyscallExecutorBaseError::from)?;

    // Keccak resource usage depends on the input length, so we increment the syscall count
    // in the syscall execution callback.
    if selector != SyscallSelector::Keccak {
        syscall_executor.increment_syscall_count(&selector);
    }

    execute_syscall_from_selector(syscall_executor, vm, selector)
}

pub enum SelfOrRevert<T> {
    Original(T),
    Revert(RevertData),
}

impl<T> SelfOrRevert<T> {
    pub fn map_original<F, U>(self, f: F) -> SelfOrRevert<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            SelfOrRevert::Original(val) => SelfOrRevert::Original(f(val)),
            SelfOrRevert::Revert(data) => SelfOrRevert::Revert(data),
        }
    }
}

pub trait TryExtractRevert {
    fn try_extract_revert(self) -> SelfOrRevert<Self>
    where
        Self: Sized;

    fn as_revert(revert_data: RevertData) -> Self;

    fn from_self_or_revert(self_or_revert: SelfOrRevert<Self>) -> Self
    where
        Self: Sized,
    {
        match self_or_revert {
            SelfOrRevert::Original(original) => original,
            SelfOrRevert::Revert(data) => Self::as_revert(data),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyscallExecutorBaseError {
    #[error(transparent)]
    DeprecatedSyscallExecutorBase(#[from] DeprecatedSyscallExecutorBaseError),
    #[error(transparent)]
    FromStr(#[from] FromStrError),
    #[error("Failed to get gas cost for syscall selector {selector:?}. Error: {error:?}")]
    GasCost { error: GasCostsError, selector: SyscallSelector },
    #[error(transparent)]
    Hint(#[from] HintError),
    #[error("Invalid address domain: {address_domain}.")]
    InvalidAddressDomain { address_domain: Felt },
    #[error("Unauthorized syscall {syscall_name} in execution mode {execution_mode}.")]
    InvalidSyscallInExecutionMode { syscall_name: String, execution_mode: ExecutionMode },
    #[error("Invalid syscall input: {input:?}; {info}")]
    InvalidSyscallInput { input: Felt, info: String },
    #[error(transparent)]
    Math(#[from] MathError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error(transparent)]
    SierraType(#[from] SierraTypeError),
    #[error(transparent)]
    StarknetApi(#[from] StarknetApiError),
    #[error(transparent)]
    VirtualMachine(#[from] VirtualMachineError),
    #[error("Syscall revert.")]
    Revert { error_data: Vec<Felt> },
}

pub type SyscallBaseResult<T> = Result<T, SyscallExecutorBaseError>;

// Needed for custom hint implementations (in our case, syscall hints) which must comply with the
// cairo-rs API.
impl From<SyscallExecutorBaseError> for HintError {
    fn from(error: SyscallExecutorBaseError) -> Self {
        Self::Internal(VirtualMachineError::Other(error.into()))
    }
}

impl TryExtractRevert for SyscallExecutorBaseError {
    fn try_extract_revert(self) -> SelfOrRevert<Self>
    where
        Self: Sized,
    {
        match self {
            Self::Revert { error_data } => SelfOrRevert::Revert(RevertData::new_normal(error_data)),
            _ => SelfOrRevert::Original(self),
        }
    }

    fn as_revert(revert_data: RevertData) -> Self {
        Self::Revert { error_data: revert_data.error_data }
    }
}

#[derive(Debug)]
pub struct RevertData {
    pub error_data: Vec<Felt>,
    use_temp_segment: bool,
}

impl RevertData {
    pub fn new_temp(error_data: Vec<Felt>) -> Self {
        RevertData { error_data, use_temp_segment: true }
    }

    pub fn new_normal(error_data: Vec<Felt>) -> Self {
        RevertData { error_data, use_temp_segment: false }
    }
}
