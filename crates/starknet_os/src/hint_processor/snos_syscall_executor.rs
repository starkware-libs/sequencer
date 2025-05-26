use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::execution::execution_utils::ReadOnlySegment;
use blockifier::execution::syscalls::hint_processor::SyscallExecutionError;
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
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::execution_resources::GasAmount;

use crate::hint_processor::execution_helper::ExecutionHelperError;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;

#[derive(Debug, thiserror::Error)]
pub enum SnosSyscallError {
    #[error(transparent)]
    SyscallExecutorBase(#[from] SyscallExecutorBaseError),
}

impl TryExtractRevert for SnosSyscallError {
    fn try_extract_revert(self) -> SelfOrRevert<Self> {
        // No revert case in this error enum.
        SelfOrRevert::Original(self)
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

    fn call_contract(
        request: CallContractRequest,
        vm: &mut VirtualMachine,
        syscall_handler: &mut Self,
        remaining_gas: &mut u64,
    ) -> SyscallResult<CallContractResponse> {
        // TODO(Tzahi): Change `expect`s to regular errors once the syscall trait has an associated
        // error type.
        let call_tracker = syscall_handler
            .execution_helpers_manager
            .get_mut_current_execution_helper()
            .expect("No current execution helper")
            .tx_execution_iter
            .get_mut_tx_execution_info_ref()
            .expect("No current tx execution info")
            .call_info_tracker
            .as_mut()
            .expect("No call info tracker found");

        let next_call_execution = &call_tracker
            .inner_calls_iterator
            .next()
            .ok_or(ExecutionHelperError::MissingCallInfo)
            .expect("Missing call info")
            .execution;

        *remaining_gas -= next_call_execution.gas_consumed;
        let ret_data = &next_call_execution.retdata.0;

        if next_call_execution.failed {
            return Err(SyscallExecutionError::Revert { error_data: ret_data.clone() });
        };

        let relocatable_ret_data: Vec<MaybeRelocatable> =
            ret_data.iter().map(|&x| MaybeRelocatable::from(x)).collect();

        let retdata_segment_start_ptr = vm.add_memory_segment();
        vm.load_data(retdata_segment_start_ptr, &relocatable_ret_data)?;

        Ok(CallContractResponse {
            segment: ReadOnlySegment {
                start_ptr: retdata_segment_start_ptr,
                length: relocatable_ret_data.len(),
            },
        })
    }

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
        Ok(DeployResponse {
            contract_address: deployed_contract_address,
            constructor_retdata: ReadOnlySegment { start_ptr: retdata_base, length: retdata.len() },
        })
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

    fn versioned_constants(&self) -> &VersionedConstants {
        VersionedConstants::latest_constants()
    }
}
