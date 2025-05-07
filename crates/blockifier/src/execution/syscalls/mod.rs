use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use num_traits::ToPrimitive;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt, TransactionSignature};
use starknet_api::transaction::{EventContent, EventData, EventKey, L2ToL1Payload};
use starknet_types_core::felt::Felt;
use syscall_executor::SyscallExecutorBaseError;

use self::hint_processor::{
    felt_to_bool,
    read_call_params,
    read_calldata,
    read_felt_array,
    write_segment,
    EmitEventError,
    SyscallExecutionError,
};
use crate::blockifier_versioned_constants::{EventLimits, VersionedConstants};
use crate::execution::call_info::MessageToL1;
use crate::execution::deprecated_syscalls::DeprecatedSyscallSelector;
use crate::execution::execution_utils::{
    felt_from_ptr,
    write_felt,
    write_maybe_relocatable,
    ReadOnlySegment,
};
use crate::execution::syscalls::syscall_base::SyscallResult;

pub mod hint_processor;
pub mod secp;
pub mod syscall_base;
pub mod syscall_executor;

#[cfg(test)]
pub mod syscall_tests;

pub type WriteResponseResult = SyscallResult<()>;

pub type SyscallSelector = DeprecatedSyscallSelector;

pub trait SyscallRequest: Sized {
    fn read(_vm: &VirtualMachine, _ptr: &mut Relocatable) -> SyscallResult<Self>;

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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<Self> {
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
    Failure { gas_counter: u64, error_data: Vec<Felt> },
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
            Self::Failure { gas_counter, error_data } => {
                write_felt(vm, ptr, Felt::from(gas_counter))?;
                // 1 to indicate failure.
                write_felt(vm, ptr, Felt::ONE)?;

                // Write the error data to a new memory segment.
                let revert_reason_start = vm.add_memory_segment();
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
    fn read(_vm: &VirtualMachine, _ptr: &mut Relocatable) -> SyscallResult<EmptyRequest> {
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
    segment: ReadOnlySegment,
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<CallContractRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<DeployRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<EmitEventRequest> {
        let keys =
            read_felt_array::<SyscallExecutionError>(vm, ptr)?.into_iter().map(EventKey).collect();
        let data = EventData(read_felt_array::<SyscallExecutionError>(vm, ptr)?);

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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<GetBlockHashRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<LibraryCallRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<MetaTxV0Request> {
        let contract_address = ContractAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let (entry_point_selector, calldata) = read_call_params(vm, ptr)?;
        let signature =
            TransactionSignature(read_felt_array::<SyscallExecutionError>(vm, ptr)?.into());

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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<ReplaceClassRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<SendMessageToL1Request> {
        let to_address = EthAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let payload = L2ToL1Payload(read_felt_array::<SyscallExecutionError>(vm, ptr)?);

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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<StorageReadRequest> {
        let address_domain = felt_from_ptr(vm, ptr)?;
        if address_domain != Felt::ZERO {
            return Err(SyscallExecutionError::InvalidAddressDomain { address_domain });
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<StorageWriteRequest> {
        let address_domain = felt_from_ptr(vm, ptr)?;
        if address_domain != Felt::ZERO {
            return Err(SyscallExecutionError::InvalidAddressDomain { address_domain });
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<KeccakRequest> {
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
    ) -> SyscallResult<Sha256ProcessBlockRequest> {
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
    fn read(vm: &VirtualMachine, ptr: &mut Relocatable) -> SyscallResult<GetClassHashAtRequest> {
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
