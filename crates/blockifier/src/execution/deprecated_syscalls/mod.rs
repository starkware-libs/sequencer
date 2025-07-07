use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use deprecated_syscall_executor::{
    DeprecatedSyscallExecutorBaseError,
    DeprecatedSyscallExecutorBaseResult,
};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::{EventContent, EventData, EventKey, L2ToL1Payload};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use self::hint_processor::{
    felt_to_bool,
    read_call_params,
    read_calldata,
    read_felt_array,
    DeprecatedSyscallExecutionError,
};
use crate::execution::call_info::MessageToL1;
use crate::execution::execution_utils::{
    felt_from_ptr,
    write_felt,
    write_maybe_relocatable,
    ReadOnlySegment,
};

pub mod deprecated_syscall_executor;
#[cfg(test)]
#[path = "deprecated_syscalls_test.rs"]
pub mod deprecated_syscalls_test;
pub mod hint_processor;

pub type DeprecatedSyscallResult<T> = Result<T, DeprecatedSyscallExecutionError>;
pub type WriteResponseResult = DeprecatedSyscallExecutorBaseResult<()>;

#[derive(Clone, Copy, Debug, Deserialize, EnumIter, Eq, Hash, PartialEq, Serialize)]
pub enum DeprecatedSyscallSelector {
    CallContract,
    DelegateCall,
    DelegateL1Handler,
    Deploy,
    EmitEvent,
    GetBlockHash,
    GetBlockNumber,
    GetBlockTimestamp,
    GetCallerAddress,
    GetClassHashAt,
    GetContractAddress,
    GetExecutionInfo,
    GetSequencerAddress,
    GetTxInfo,
    GetTxSignature,
    Keccak,
    // TODO(Noa): Remove it (as it is not a syscall) and define its resources in
    // `OsResources`.
    KeccakRound,
    Sha256ProcessBlock,
    LibraryCall,
    LibraryCallL1Handler,
    MetaTxV0,
    ReplaceClass,
    Secp256k1Add,
    Secp256k1GetPointFromX,
    Secp256k1GetXy,
    Secp256k1Mul,
    Secp256k1New,
    Secp256r1Add,
    Secp256r1GetPointFromX,
    Secp256r1GetXy,
    Secp256r1Mul,
    Secp256r1New,
    SendMessageToL1,
    StorageRead,
    StorageWrite,
}

impl DeprecatedSyscallSelector {
    pub fn is_calling_syscall(&self) -> bool {
        matches!(
            self,
            Self::CallContract
                | Self::DelegateCall
                | Self::DelegateL1Handler
                | Self::Deploy
                | Self::LibraryCall
                | Self::LibraryCallL1Handler
                | Self::MetaTxV0
        )
    }
}

impl TryFrom<Felt> for DeprecatedSyscallSelector {
    type Error = DeprecatedSyscallExecutorBaseError;
    fn try_from(raw_selector: Felt) -> Result<Self, Self::Error> {
        // Remove leading zero bytes from selector.
        let selector_bytes = raw_selector.to_bytes_be();
        let first_non_zero = selector_bytes.iter().position(|&byte| byte != b'\0').unwrap_or(32);

        match &selector_bytes[first_non_zero..] {
            b"CallContract" => Ok(Self::CallContract),
            b"DelegateCall" => Ok(Self::DelegateCall),
            b"DelegateL1Handler" => Ok(Self::DelegateL1Handler),
            b"Deploy" => Ok(Self::Deploy),
            b"EmitEvent" => Ok(Self::EmitEvent),
            b"GetBlockHash" => Ok(Self::GetBlockHash),
            b"GetBlockNumber" => Ok(Self::GetBlockNumber),
            b"GetBlockTimestamp" => Ok(Self::GetBlockTimestamp),
            b"GetCallerAddress" => Ok(Self::GetCallerAddress),
            b"GetClassHashAt" => Ok(Self::GetClassHashAt),
            b"GetContractAddress" => Ok(Self::GetContractAddress),
            b"GetExecutionInfo" => Ok(Self::GetExecutionInfo),
            b"GetSequencerAddress" => Ok(Self::GetSequencerAddress),
            b"GetTxInfo" => Ok(Self::GetTxInfo),
            b"GetTxSignature" => Ok(Self::GetTxSignature),
            b"Keccak" => Ok(Self::Keccak),
            b"Sha256ProcessBlock" => Ok(Self::Sha256ProcessBlock),
            b"LibraryCall" => Ok(Self::LibraryCall),
            b"LibraryCallL1Handler" => Ok(Self::LibraryCallL1Handler),
            b"MetaTxV0" => Ok(Self::MetaTxV0),
            b"ReplaceClass" => Ok(Self::ReplaceClass),
            b"Secp256k1Add" => Ok(Self::Secp256k1Add),
            b"Secp256k1GetPointFromX" => Ok(Self::Secp256k1GetPointFromX),
            b"Secp256k1GetXy" => Ok(Self::Secp256k1GetXy),
            b"Secp256k1Mul" => Ok(Self::Secp256k1Mul),
            b"Secp256k1New" => Ok(Self::Secp256k1New),
            b"Secp256r1Add" => Ok(Self::Secp256r1Add),
            b"Secp256r1GetPointFromX" => Ok(Self::Secp256r1GetPointFromX),
            b"Secp256r1GetXy" => Ok(Self::Secp256r1GetXy),
            b"Secp256r1Mul" => Ok(Self::Secp256r1Mul),
            b"Secp256r1New" => Ok(Self::Secp256r1New),
            b"SendMessageToL1" => Ok(Self::SendMessageToL1),
            b"StorageRead" => Ok(Self::StorageRead),
            b"StorageWrite" => Ok(Self::StorageWrite),
            _ => Err(DeprecatedSyscallExecutorBaseError::InvalidDeprecatedSyscallSelector(
                raw_selector,
            )),
        }
    }
}

pub trait SyscallRequest: Sized {
    fn read(
        _vm: &VirtualMachine,
        _ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<Self>;
}

pub trait SyscallResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, _vm: &mut VirtualMachine, _ptr: &mut Relocatable) -> WriteResponseResult;
}

// Common structs.

#[derive(Debug, Eq, PartialEq)]
pub struct EmptyRequest;

impl SyscallRequest for EmptyRequest {
    fn read(
        _vm: &VirtualMachine,
        _ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<EmptyRequest> {
        Ok(EmptyRequest)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct EmptyResponse;

impl SyscallResponse for EmptyResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, _vm: &mut VirtualMachine, _ptr: &mut Relocatable) -> WriteResponseResult {
        Ok(())
    }
}

#[derive(Debug)]
pub struct SingleSegmentResponse {
    pub segment: ReadOnlySegment,
}

impl SyscallResponse for SingleSegmentResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, self.segment.length)?;
        write_maybe_relocatable(vm, ptr, self.segment.start_ptr)?;
        Ok(())
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
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<CallContractRequest> {
        let contract_address = ContractAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let (function_selector, calldata) = read_call_params(vm, ptr)?;

        Ok(CallContractRequest { contract_address, function_selector, calldata })
    }
}

pub type CallContractResponse = SingleSegmentResponse;

// DelegateCall and DelegateCallL1Handler syscalls.

pub type DelegateCallRequest = CallContractRequest;
pub type DelegateCallResponse = CallContractResponse;

// Deploy syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct DeployRequest {
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub deploy_from_zero: bool,
}

impl SyscallRequest for DeployRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<DeployRequest> {
        let class_hash = ClassHash(felt_from_ptr(vm, ptr)?);
        let contract_address_salt = ContractAddressSalt(felt_from_ptr(vm, ptr)?);
        let constructor_calldata = read_calldata(vm, ptr)?;
        let deploy_from_zero = felt_from_ptr(vm, ptr)?;

        Ok(DeployRequest {
            class_hash,
            contract_address_salt,
            constructor_calldata,
            deploy_from_zero: felt_to_bool(deploy_from_zero)?,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DeployResponse {
    pub contract_address: ContractAddress,
}

impl SyscallResponse for DeployResponse {
    // The Cairo struct contains: `contract_address`, `constructor_retdata_size`,
    // `constructor_retdata`.
    // Nonempty constructor retdata is currently not supported.
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, *self.contract_address.0.key())?;
        write_maybe_relocatable(vm, ptr, 0)?;
        write_maybe_relocatable(vm, ptr, 0)?;
        Ok(())
    }
}

// EmitEvent syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct EmitEventRequest {
    pub content: EventContent,
}

impl SyscallRequest for EmitEventRequest {
    // The Cairo struct contains: `keys_len`, `keys`, `data_len`, `data`·
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<EmitEventRequest> {
        let keys = read_felt_array::<DeprecatedSyscallExecutorBaseError>(vm, ptr)?
            .into_iter()
            .map(EventKey)
            .collect();
        let data = EventData(read_felt_array::<DeprecatedSyscallExecutorBaseError>(vm, ptr)?);

        Ok(EmitEventRequest { content: EventContent { keys, data } })
    }
}

pub type EmitEventResponse = EmptyResponse;

// GetBlockNumber syscall.

pub type GetBlockNumberRequest = EmptyRequest;

#[derive(Debug, Eq, PartialEq)]
pub struct GetBlockNumberResponse {
    pub block_number: BlockNumber,
}

impl SyscallResponse for GetBlockNumberResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, Felt::from(self.block_number.0))?;
        Ok(())
    }
}

// GetBlockTimestamp syscall.

pub type GetBlockTimestampRequest = EmptyRequest;

#[derive(Debug, Eq, PartialEq)]
pub struct GetBlockTimestampResponse {
    pub block_timestamp: BlockTimestamp,
}

impl SyscallResponse for GetBlockTimestampResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, Felt::from(self.block_timestamp.0))?;
        Ok(())
    }
}

// GetCallerAddress syscall.

pub type GetCallerAddressRequest = EmptyRequest;
pub type GetCallerAddressResponse = GetContractAddressResponse;

// GetContractAddress syscall.

pub type GetContractAddressRequest = EmptyRequest;

#[derive(Debug, Eq, PartialEq)]
pub struct GetContractAddressResponse {
    pub address: ContractAddress,
}

impl SyscallResponse for GetContractAddressResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, *self.address.0.key())?;
        Ok(())
    }
}

// GetSequencerAddress syscall.

pub type GetSequencerAddressRequest = EmptyRequest;
pub type GetSequencerAddressResponse = GetContractAddressResponse;

// GetTxInfo syscall.

pub type GetTxInfoRequest = EmptyRequest;

#[derive(Debug, Eq, PartialEq)]
pub struct GetTxInfoResponse {
    pub tx_info_start_ptr: Relocatable,
}

impl SyscallResponse for GetTxInfoResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_maybe_relocatable(vm, ptr, self.tx_info_start_ptr)?;
        Ok(())
    }
}

// GetTxSignature syscall.

pub type GetTxSignatureRequest = EmptyRequest;
pub type GetTxSignatureResponse = SingleSegmentResponse;

// LibraryCall and LibraryCallL1Handler syscalls.

#[derive(Debug, Eq, PartialEq)]
pub struct LibraryCallRequest {
    pub class_hash: ClassHash,
    pub function_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl SyscallRequest for LibraryCallRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<LibraryCallRequest> {
        let class_hash = ClassHash(felt_from_ptr(vm, ptr)?);
        let (function_selector, calldata) = read_call_params(vm, ptr)?;

        Ok(LibraryCallRequest { class_hash, function_selector, calldata })
    }
}

pub type LibraryCallResponse = CallContractResponse;

// ReplaceClass syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct ReplaceClassRequest {
    pub class_hash: ClassHash,
}

impl SyscallRequest for ReplaceClassRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<ReplaceClassRequest> {
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
    ) -> DeprecatedSyscallExecutorBaseResult<SendMessageToL1Request> {
        let to_address = EthAddress::try_from(felt_from_ptr(vm, ptr)?)?;
        let payload =
            L2ToL1Payload(read_felt_array::<DeprecatedSyscallExecutorBaseError>(vm, ptr)?);

        Ok(SendMessageToL1Request { message: MessageToL1 { to_address, payload } })
    }
}

pub type SendMessageToL1Response = EmptyResponse;

// StorageRead syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct StorageReadRequest {
    pub address: StorageKey,
}

impl SyscallRequest for StorageReadRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<StorageReadRequest> {
        let address = StorageKey::try_from(felt_from_ptr(vm, ptr)?)?;
        Ok(StorageReadRequest { address })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct StorageReadResponse {
    pub value: Felt,
}

impl SyscallResponse for StorageReadResponse {
    #[allow(clippy::result_large_err)]
    fn write(self, vm: &mut VirtualMachine, ptr: &mut Relocatable) -> WriteResponseResult {
        write_felt(vm, ptr, self.value)?;
        Ok(())
    }
}

// StorageWrite syscall.

#[derive(Debug, Eq, PartialEq)]
pub struct StorageWriteRequest {
    pub address: StorageKey,
    pub value: Felt,
}

impl SyscallRequest for StorageWriteRequest {
    fn read(
        vm: &VirtualMachine,
        ptr: &mut Relocatable,
    ) -> DeprecatedSyscallExecutorBaseResult<StorageWriteRequest> {
        let address = StorageKey::try_from(felt_from_ptr(vm, ptr)?)?;
        let value = felt_from_ptr(vm, ptr)?;
        Ok(StorageWriteRequest { address, value })
    }
}

pub type StorageWriteResponse = EmptyResponse;
