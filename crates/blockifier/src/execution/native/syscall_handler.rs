use std::collections::HashSet;
use std::hash::RandomState;
use std::sync::Arc;

use cairo_native::starknet::{
    ExecutionInfo,
    ExecutionInfoV2,
    Secp256k1Point,
    Secp256r1Point,
    StarknetSyscallHandler,
    SyscallResult,
    U256,
};
use cairo_native::starknet_stub::encode_str_as_felts;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message, Retdata};
use crate::execution::entry_point::{
    CallEntryPoint,
    ConstructorContext,
    EntryPointExecutionContext,
};
use crate::execution::execution_utils::execute_deployment;
use crate::execution::syscalls::hint_processor::{
    SyscallCounter,
    SyscallExecutionError,
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
            resources,
            context,
            events: Vec::new(),
            l2_to_l1_messages: Vec::new(),
            inner_calls: Vec::new(),
            syscall_counter: SyscallCounter::new(),
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
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        let retdata = call_info.execution.retdata.clone();

        if call_info.execution.failed {
            let error = SyscallExecutionError::SyscallError { error_data: retdata.0 };
            return Err(self.handle_error(remaining_gas, error));
        }

        // TODO(Noa, 1/11/2024): remove this once the gas type is u64.
        // Change the remaining gas value.
        *remaining_gas = u128::from(remaining_gas_u64);

        self.inner_calls.push(call_info);

        Ok(retdata)
    }

    fn handle_error(
        &mut self,
        _remaining_gas: &mut u128,
        error: SyscallExecutionError,
    ) -> Vec<Felt> {
        match error {
            SyscallExecutionError::SyscallError { error_data } => error_data,
            // unrecoverable errors are yet to be implemented
            _ => encode_str_as_felts(&error.to_string()),
        }
    }

    /// Handles all gas-related logics and additional metadata such as `SyscallCounter`. In native,
    /// we need to explicitly call this method at the beginning of each syscall.
    fn pre_execute_syscall(
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
        _block_number: u64,
        _remaining_gas: &mut u128,
    ) -> SyscallResult<Felt> {
        todo!("Implement get_block_hash syscall.");
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
        class_hash: Felt,
        contract_address_salt: Felt,
        calldata: &[Felt],
        deploy_from_zero: bool,
        remaining_gas: &mut u128,
    ) -> SyscallResult<(Felt, Vec<Felt>)> {
        self.pre_execute_syscall(
            remaining_gas,
            SyscallSelector::Deploy,
            self.context.gas_costs().deploy_gas_cost,
        )?;

        let deployer_address = self.call.storage_address;
        let deployer_address_for_calculation =
            if deploy_from_zero { ContractAddress::default() } else { deployer_address };

        let class_hash = ClassHash(class_hash);
        let calldata = Calldata(Arc::new(calldata.to_vec()));

        let deployed_contract_address = calculate_contract_address(
            ContractAddressSalt(contract_address_salt),
            class_hash,
            &calldata,
            deployer_address_for_calculation,
        )
        .map_err(|err| self.handle_error(remaining_gas, err.into()))?;

        let ctor_context = ConstructorContext {
            class_hash,
            code_address: Some(deployed_contract_address),
            storage_address: deployed_contract_address,
            caller_address: deployer_address,
        };

        let mut remaining_gas_u64 =
            u64::try_from(*remaining_gas).expect("Failed to convert gas to u64.");

        let call_info = execute_deployment(
            self.state,
            self.resources,
            self.context,
            ctor_context,
            calldata,
            // Warning: converting of reference would create a new reference to different data,
            // example:
            //     let mut a: u128 = 1;
            //     let a_ref: &mut u128 = &mut a;
            //
            //     let mut b: u64 = u64::try_from(*a_ref).unwrap();
            //
            //     assert_eq!(b, 1);
            //
            //     b += 1;
            //
            //     assert_eq!(b, 2);
            //     assert_eq!(a, 1);
            &mut remaining_gas_u64,
        )
        .map_err(|err| self.handle_error(remaining_gas, err.into()))?;

        *remaining_gas = u128::from(remaining_gas_u64);

        let constructor_retdata = call_info.execution.retdata.0[..].to_vec();

        self.inner_calls.push(call_info);

        Ok((Felt::from(deployed_contract_address), constructor_retdata))
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
        address_domain: u32,
        address: Felt,
        remaining_gas: &mut u128,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(
            remaining_gas,
            SyscallSelector::StorageRead,
            self.context.gas_costs().storage_read_gas_cost,
        )?;

        if address_domain != 0 {
            let address_domain = Felt::from(address_domain);
            let error = SyscallExecutionError::InvalidAddressDomain { address_domain };
            return Err(self.handle_error(remaining_gas, error));
        }

        let key = StorageKey::try_from(address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        let read_result = self.state.get_storage_at(self.call.storage_address, key);
        let value = read_result.map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        self.accessed_keys.insert(key);
        self.read_values.push(value);

        Ok(value)
    }

    fn storage_write(
        &mut self,
        address_domain: u32,
        address: Felt,
        value: Felt,
        remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(
            remaining_gas,
            SyscallSelector::StorageWrite,
            self.context.gas_costs().storage_write_gas_cost,
        )?;

        if address_domain != 0 {
            let address_domain = Felt::from(address_domain);
            let error = SyscallExecutionError::InvalidAddressDomain { address_domain };
            return Err(self.handle_error(remaining_gas, error));
        }

        let key = StorageKey::try_from(address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        self.accessed_keys.insert(key);

        let write_result = self.state.set_storage_at(self.call.storage_address, key, value);
        write_result.map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        Ok(())
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
        _prev_state: &mut [u32; 8],
        _current_block: &[u32; 16],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        todo!("Implement sha256_process_block syscall.");
    }
}
