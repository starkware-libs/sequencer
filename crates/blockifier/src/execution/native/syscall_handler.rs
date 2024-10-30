use std::collections::HashSet;
use std::hash::RandomState;

use cairo_native::starknet::{
    ExecutionInfo,
    ExecutionInfoV2,
    Secp256k1Point,
    Secp256r1Point,
    StarknetSyscallHandler,
    SyscallResult,
    U256,
};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message, Retdata};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::{CallEntryPoint, EntryPointExecutionContext};
use crate::execution::native::utils::encode_str_as_felts;
use crate::execution::syscalls::hint_processor::{
    SyscallCounter,
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
    OUT_OF_GAS_ERROR,
};
use crate::execution::syscalls::SyscallSelector;
use crate::state::state_api::State;

pub struct NativeSyscallHandler<'state> {
    // Input for execution.
    pub state: &'state mut dyn State,
    pub resources: &'state mut ExecutionResources,
    pub context: &'state mut EntryPointExecutionContext,
    pub call: CallEntryPoint,

    // Execution results.
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub inner_calls: Vec<CallInfo>,

    pub syscall_counter: SyscallCounter,

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey, RandomState>,
}

impl<'state> NativeSyscallHandler<'state> {
    pub fn new(
        call: CallEntryPoint,
        state: &'state mut dyn State,
        resources: &'state mut ExecutionResources,
        context: &'state mut EntryPointExecutionContext,
    ) -> NativeSyscallHandler<'state> {
        NativeSyscallHandler {
            state,
            call,
            syscall_counter: SyscallCounter::new(),
            resources,
            context,
            events: Vec::new(),
            l2_to_l1_messages: Vec::new(),
            inner_calls: Vec::new(),
            read_values: Vec::new(),
            accessed_keys: HashSet::new(),
        }
    }

    #[allow(dead_code)]
    fn increment_syscall_count_by(&mut self, selector: &SyscallSelector, n: usize) {
        let syscall_count = self.syscall_counter.entry(*selector).or_default();
        *syscall_count += n
    }

    #[allow(dead_code)]
    fn execute_inner_call(
        &mut self,
        entry_point: CallEntryPoint,
        remaining_gas: &mut u128,
    ) -> SyscallResult<Retdata> {
        let mut remaining_gas_u64 =
            u64::try_from(*remaining_gas).expect("Failed to convert gas to u64.");
        let call_info = entry_point
            .execute(self.state, self.resources, self.context, &mut remaining_gas_u64)
            .map_err(|e| encode_str_as_felts(&e.to_string()))?;
        let retdata = call_info.execution.retdata.clone();

        if call_info.execution.failed {
            // In VM it's wrapped into `SyscallExecutionError::SyscallError`.
            return Err(retdata.0.clone());
        }

        // TODO(Noa, 1/11/2024): remove this once the gas type is u64.
        // Change the remaining gas value.
        *remaining_gas = u128::from(remaining_gas_u64);

        self.inner_calls.push(call_info);

        Ok(retdata)
    }

    // Handles gas related logic when executing a syscall. Required because Native calls the
    // syscalls directly unlike the VM where the `execute_syscall` method perform this operation
    // first.
    #[allow(dead_code)]
    fn substract_syscall_gas_cost(
        &mut self,
        remaining_gas: &mut u128,
        syscall_selector: SyscallSelector,
        syscall_gas_cost: u64,
    ) -> SyscallResult<()> {
        // Syscall count for Keccak is done differently
        if syscall_selector != SyscallSelector::Keccak {
            self.increment_syscall_count_by(&syscall_selector, 1);
        }

        // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
        let required_gas =
            u128::from(syscall_gas_cost - self.context.gas_costs().syscall_base_gas_cost);

        if *remaining_gas < required_gas {
            // Out of gas failure.
            return Err(vec![
                Felt::from_hex(OUT_OF_GAS_ERROR)
                    .expect("Failed to parse OUT_OF_GAS_ERROR hex string"),
            ]);
        }

        *remaining_gas -= required_gas;

        Ok(())
    }
}

impl<'state> StarknetSyscallHandler for &mut NativeSyscallHandler<'state> {
    fn get_block_hash(
        &mut self,
        block_number: u64,
        remaining_gas: &mut u128,
    ) -> SyscallResult<Felt> {
        self.substract_syscall_gas_cost(
            remaining_gas,
            SyscallSelector::GetBlockHash,
            self.context.gas_costs().get_block_hash_gas_cost,
        )?;

        if self.context.execution_mode == ExecutionMode::Validate {
            let err = SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "get_block_hash".to_string(),
                execution_mode: ExecutionMode::Validate,
            };

            return Err(encode_str_as_felts(&err.to_string()));
        }

        let current_block_number =
            self.context.tx_context.block_context.block_info().block_number.0;

        if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
            || block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
        {
            // `panic` is unreachable in this case, also this is covered by tests so we can safely
            // unwrap
            let out_of_range_felt = Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR).unwrap();

            // This error is wrapped into a `SyscallExecutionError::SyscallError` in the VM
            // implementation, but here it would be more convenient to return it directly, since
            // wrapping it like VM does will result in a double encoding to felts, which adds extra
            // layer of complication
            return Err(vec![out_of_range_felt]);
        }

        let key = StorageKey::try_from(Felt::from(block_number))
            .expect("Failed to get storage key for block: {block_number}.");
        let block_hash_address =
            ContractAddress::try_from(Felt::from(constants::BLOCK_HASH_CONTRACT_ADDRESS))
                .map_err(|e| encode_str_as_felts(&e.to_string()))?;

        match self.state.get_storage_at(block_hash_address, key) {
            Ok(value) => Ok(value),
            Err(e) => Err(encode_str_as_felts(&e.to_string())),
        }
    }

    fn get_execution_info(&mut self, _remaining_gas: &mut u128) -> SyscallResult<ExecutionInfo> {
        todo!("Implement get_execution_info syscall.");
    }

    fn get_execution_info_v2(
        &mut self,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<ExecutionInfoV2> {
        todo!("Implement get_execution_info_v2 syscall.");
    }

    fn deploy(
        &mut self,
        _class_hash: Felt,
        _contract_address_salt: Felt,
        _calldata: &[Felt],
        _deploy_from_zero: bool,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<(Felt, Vec<Felt>)> {
        todo!("Implement deploy syscall.");
    }

    fn replace_class(&mut self, _class_hash: Felt, _remaining_gas: &mut u128) -> SyscallResult<()> {
        todo!("Implement replace_class syscall.");
    }

    fn library_call(
        &mut self,
        _class_hash: Felt,
        _function_selector: Felt,
        _calldata: &[Felt],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Vec<Felt>> {
        todo!("Implement library_call syscall.");
    }

    fn call_contract(
        &mut self,
        _address: Felt,
        _entry_point_selector: Felt,
        _calldata: &[Felt],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Vec<Felt>> {
        todo!("Implement call_contract syscall.");
    }

    fn storage_read(
        &mut self,
        _address_domain: u32,
        _address: Felt,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Felt> {
        todo!("Implement storage_read syscall.");
    }

    fn storage_write(
        &mut self,
        _address_domain: u32,
        _address: Felt,
        _value: Felt,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        todo!("Implement storage_write syscall.");
    }

    fn emit_event(
        &mut self,
        _keys: &[Felt],
        _data: &[Felt],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        todo!("Implement emit_event syscall.");
    }

    fn send_message_to_l1(
        &mut self,
        _to_address: Felt,
        _payload: &[Felt],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        todo!("Implement send_message_to_l1 syscall.");
    }

    fn keccak(&mut self, _input: &[u64], _remaining_gas: &mut u128) -> SyscallResult<U256> {
        todo!("Implement keccak syscall.");
    }

    fn secp256k1_new(
        &mut self,
        _x: U256,
        _y: U256,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Option<Secp256k1Point>> {
        todo!("Implement secp256k1_new syscall.");
    }

    fn secp256k1_add(
        &mut self,
        _p0: Secp256k1Point,
        _p1: Secp256k1Point,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Secp256k1Point> {
        todo!("Implement secp256k1_add syscall.");
    }

    fn secp256k1_mul(
        &mut self,
        _p: Secp256k1Point,
        _m: U256,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Secp256k1Point> {
        todo!("Implement secp256k1_mul syscall.");
    }

    fn secp256k1_get_point_from_x(
        &mut self,
        _x: U256,
        _y_parity: bool,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Option<Secp256k1Point>> {
        todo!("Implement secp256k1_get_point_from_x syscall.");
    }

    fn secp256k1_get_xy(
        &mut self,
        _p: Secp256k1Point,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<(U256, U256)> {
        todo!("Implement secp256k1_get_xy syscall.");
    }

    fn secp256r1_new(
        &mut self,
        _x: U256,
        _y: U256,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Option<Secp256r1Point>> {
        todo!("Implement secp256r1_new syscall.");
    }

    fn secp256r1_add(
        &mut self,
        _p0: Secp256r1Point,
        _p1: Secp256r1Point,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Secp256r1Point> {
        todo!("Implement secp256r1_add syscall.");
    }

    fn secp256r1_mul(
        &mut self,
        _p: Secp256r1Point,
        _m: U256,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Secp256r1Point> {
        todo!("Implement secp256r1_mul syscall.");
    }

    fn secp256r1_get_point_from_x(
        &mut self,
        _x: U256,
        _y_parity: bool,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Option<Secp256r1Point>> {
        todo!("Implement secp256r1_get_point_from_x syscall.");
    }

    fn secp256r1_get_xy(
        &mut self,
        _p: Secp256r1Point,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<(U256, U256)> {
        todo!("Implement secp256r1_get_xy syscall.");
    }

    fn sha256_process_block(
        &mut self,
        _prev_state: &[u32; 8],
        _current_block: &[u32; 16],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<[u32; 8]> {
        todo!("Implement sha256_process_block syscall.");
    }
}
