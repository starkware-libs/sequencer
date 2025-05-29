use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::execution::execution_utils::ReadOnlySegment;
use blockifier::execution::syscalls::hint_processor::{SyscallExecutionError, INVALID_ARGUMENT};
use blockifier::execution::syscalls::secp::SecpHintProcessor;
use blockifier::execution::syscalls::syscall_base::SyscallResult;
use blockifier::execution::syscalls::syscall_executor::SyscallExecutor;
use blockifier::execution::syscalls::vm_syscall_utils::{
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
    LibraryCallRequest,
    LibraryCallResponse,
    MetaTxV0Request,
    MetaTxV0Response,
    ReplaceClassRequest,
    ReplaceClassResponse,
    SelfOrRevert,
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
use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::errors::hint_errors::HintError;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::errors::vm_errors::VirtualMachineError;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::ascii_as_felt;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::constants::EXECUTE_ENTRY_POINT_NAME;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::ExecutionHelperError;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::vm_utils::write_to_temp_segment;

#[derive(Debug, thiserror::Error)]
pub enum SnosSyscallError {
    #[error(transparent)]
    ExecutionHelper(#[from] ExecutionHelperError),
    #[error(transparent)]
    Memory(#[from] MemoryError),
    #[error("Syscall revert.")]
    Revert { error_data: Vec<Felt> },
    #[error(transparent)]
    SyscallExecutorBase(#[from] SyscallExecutorBaseError),
}

pub type SnosSyscallResult<T> = Result<T, SnosSyscallError>;

// Needed for custom hint implementations (in our case, syscall hints) which must comply with the
// cairo-rs API.
impl From<SnosSyscallError> for HintError {
    fn from(error: SnosSyscallError) -> Self {
        HintError::Internal(VirtualMachineError::Other(error.into()))
    }
}

impl TryExtractRevert for SnosSyscallError {
    fn try_extract_revert(self) -> SelfOrRevert<Self> {
        match self {
            SnosSyscallError::SyscallExecutorBase(base_error) => {
                base_error.try_extract_revert().map_original(Self::SyscallExecutorBase)
            }
            SnosSyscallError::Memory(os_hint_error) => SelfOrRevert::Original(os_hint_error.into()),
            SnosSyscallError::ExecutionHelper(execution_helper_error) => {
                SelfOrRevert::Original(execution_helper_error.into())
            }
            SnosSyscallError::Revert { error_data } => SelfOrRevert::Revert(error_data),
        }
    }

    fn as_revert(error_data: Vec<Felt>) -> Self {
        SyscallExecutorBaseError::Revert { error_data }.into()
    }
}

#[allow(unused_variables)]
impl<S: StateReader> SyscallExecutor for SnosHintProcessor<'_, S> {
    type Error = SnosSyscallError;

    fn get_keccak_round_cost_base_syscall_cost(&self) -> u64 {
        todo!()
    }

    fn get_secpk1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256k1::Config> {
        &mut self.syscall_hint_processor.secp256k1_hint_processor
    }

    fn get_secpr1_hint_processor(&mut self) -> &mut SecpHintProcessor<ark_secp256r1::Config> {
        &mut self.syscall_hint_processor.secp256r1_hint_processor
    }

    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, count: usize) {
        let syscall_usage = self.syscall_hint_processor.syscall_usage.entry(*selector).or_default();
        syscall_usage.call_count += count;
    }

    fn get_mut_syscall_ptr(&mut self) -> &mut Relocatable {
        self.syscall_hint_processor
            .get_mut_syscall_ptr()
            .expect("Syscall pointer is not initialized.")
    }

    fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount) {}

    #[allow(clippy::result_large_err)]
    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse> {
        if request.function_selector == selector_from_name(EXECUTE_ENTRY_POINT_NAME) {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![Felt::from_hex(INVALID_ARGUMENT).unwrap()],
            });
        }
        let revert_error_code = ascii_as_felt(
            &syscall_handler.versioned_constants().os_constants.error_entry_point_failed,
        )?;

        match call_contract_helper(vm, syscall_handler, remaining_gas, revert_error_code) {
            Ok(response) => Ok(response),
            Err(e) => match e.try_extract_revert() {
                SelfOrRevert::Original(e) => panic!("Error calling contract: {e}"),
                SelfOrRevert::Revert(error_data) => {
                    Err(SyscallExecutionError::Revert { error_data })
                }
            },
        }
    }

    #[allow(clippy::result_large_err)]
    fn deploy(
        request: DeployRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<DeployResponse> {
        // TODO(Nimrod): Handle errors correctly.
        let call_info_tracker = syscall_handler
            .execution_helpers_manager
            .get_mut_current_execution_helper()
            .unwrap()
            .tx_execution_iter
            .tx_execution_info_ref
            .as_mut()
            .unwrap()
            .call_info_tracker
            .as_mut()
            .unwrap();

        let deployed_contract_address =
            call_info_tracker.deployed_contracts_iterator.next().unwrap();
        let execution = &call_info_tracker.inner_calls_iterator.next().unwrap().execution;

        *remaining_gas -= execution.gas_consumed;
        let retdata: Vec<_> = execution.retdata.0.iter().map(MaybeRelocatable::from).collect();
        let retdata_base = vm.add_temporary_segment();
        vm.load_data(retdata_base, &retdata).unwrap();
        if execution.failed {
            return Err(SyscallExecutionError::Revert { error_data: execution.retdata.0.clone() });
        };
        Ok(DeployResponse {
            contract_address: deployed_contract_address,
            constructor_retdata: ReadOnlySegment { start_ptr: retdata_base, length: retdata.len() },
        })
    }

    #[allow(clippy::result_large_err)]
    fn emit_event(
        request: EmitEventRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<EmitEventResponse> {
        Ok(EmitEventResponse {})
    }

    #[allow(clippy::result_large_err)]
    fn get_block_hash(
        request: GetBlockHashRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetBlockHashResponse> {
        // TODO(Nimrod): Handle errors correctly.
        let block_number = request.block_number;
        let execution_helper = syscall_handler.get_mut_current_execution_helper().unwrap();
        let diff = execution_helper.os_block_input.block_info.block_number.0 - block_number.0;
        assert!(diff < STORED_BLOCK_HASH_BUFFER, "Block number out of range {diff}.");
        let block_hash = execution_helper
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()
            .as_mut()
            .unwrap()
            .call_info_tracker
            .as_mut()
            .unwrap()
            .execute_code_block_hash_read_iterator
            .next()
            .unwrap();

        Ok(GetBlockHashResponse { block_hash: *block_hash })
    }

    #[allow(clippy::result_large_err)]
    fn get_class_hash_at(
        request: GetClassHashAtRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetClassHashAtResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn get_execution_info(
        request: GetExecutionInfoRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<GetExecutionInfoResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn library_call(
        request: LibraryCallRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<LibraryCallResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn meta_tx_v0(
        request: MetaTxV0Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<MetaTxV0Response> {
        if request.entry_point_selector != selector_from_name(EXECUTE_ENTRY_POINT_NAME) {
            return Err(SyscallExecutionError::Revert {
                error_data: vec![Felt::from_hex(INVALID_ARGUMENT).unwrap()],
            });
        }
        // TODO(Nimrod): Handle errors correctly.
        Ok(call_contract_helper(vm, syscall_handler, remaining_gas)
            .unwrap_or_else(|e| panic!("Error calling contract: {e}")))
    }

    #[allow(clippy::result_large_err)]
    fn sha256_process_block(
        request: Sha256ProcessBlockRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Sha256ProcessBlockResponse> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn replace_class(
        request: ReplaceClassRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<ReplaceClassResponse> {
        Ok(ReplaceClassResponse {})
    }

    #[allow(clippy::result_large_err)]
    fn send_message_to_l1(
        request: SendMessageToL1Request,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<SendMessageToL1Response> {
        todo!()
    }

    #[allow(clippy::result_large_err)]
    fn storage_read(
        request: StorageReadRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageReadResponse> {
        // TODO(Tzahi): Change `expect`s to regular errors once the syscall trait has an associated
        // error type.
        assert_eq!(request.address_domain, Felt::ZERO);
        let value = *syscall_handler
            .get_mut_current_execution_helper()
            .expect("No current execution helper")
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()
            .expect("No current tx execution info")
            .get_mut_call_info_tracker()
            .expect("No call info tracker found")
            .execute_code_read_iterator
            .next()
            .expect("Missing hint for read_storage");

        Ok(StorageReadResponse { value })
    }

    #[allow(clippy::result_large_err)]
    fn storage_write(
        request: StorageWriteRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<StorageWriteResponse> {
        Ok(StorageWriteResponse {})
    }

    fn versioned_constants(&self) -> &VersionedConstants {
        VersionedConstants::latest_constants()
    }
}

#[allow(clippy::result_large_err)]
fn call_contract_helper(
    vm: &mut VirtualMachine,
    syscall_handler: &mut SnosHintProcessor<'_, impl StateReader>,
    remaining_gas: &mut u64,
    revert_error_code: Felt,
) -> SnosSyscallResult<CallContractResponse> {
    let next_call_execution = syscall_handler.get_next_call_execution();
    *remaining_gas -= next_call_execution.gas_consumed;
    let retdata = &next_call_execution.retdata.0;
    if next_call_execution.failed {
        let mut retdata = retdata.clone();
        retdata.push(revert_error_code);
        return Err(SnosSyscallError::Revert { error_data: retdata });
    };

    Ok(CallContractResponse { segment: write_to_temp_segment(retdata, vm)? })
}
