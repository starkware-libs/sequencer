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
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::core::{calculate_contract_address, ClassHash, ContractAddress};
use starknet_api::core::EthAddress;
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::L2ToL1Payload;
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{
    CallInfo,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
};
use crate::execution::entry_point::{
    CallEntryPoint,
    ConstructorContext,
    EntryPointExecutionContext,
};
use crate::execution::execution_utils::execute_deployment;
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    INVALID_INPUT_LENGTH_ERROR,
    OUT_OF_GAS_ERROR,
};
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

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey, RandomState>,

    // It is set if an unrecoverable error happens during syscall execution
    pub unrecoverable_error: Option<SyscallExecutionError>,
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
            read_values: Vec::new(),
            accessed_keys: HashSet::new(),
            unrecoverable_error: None,
        }
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

    /// Handles all gas-related logics and perform additional checks. In native,
    /// we need to explicitly call this method at the beginning of each syscall.
    fn pre_execute_syscall(
        &mut self,
        remaining_gas: &mut u128,
        syscall_gas_cost: u64,
    ) -> SyscallResult<()> {
        if self.unrecoverable_error.is_some() {
            // An unrecoverable error was found in a previous syscall, we return immediatly to
            // accelerate the end of the execution. The returned data is not important
            return Err(vec![]);
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

    fn handle_error(
        &mut self,
        remaining_gas: &mut u128,
        error: SyscallExecutionError,
    ) -> Vec<Felt> {
        // In case of more than one inner call and because each inner call has their own
        // syscall handler, if there is an unrecoverable error at call `n` it will create a
        // `NativeExecutionError`. When rolling back, each call from `n-1` to `1` will also
        // store the result of a previous `NativeExecutionError` in a `NativeExecutionError`
        // creating multiple wraps around the same error. This function is meant to prevent that.
        fn unwrap_native_error(error: SyscallExecutionError) -> SyscallExecutionError {
            match error {
                SyscallExecutionError::EntryPointExecutionError(
                    EntryPointExecutionError::NativeUnrecoverableError(e),
                ) => *e,
                _ => error,
            }
        }

        match error {
            SyscallExecutionError::SyscallError { error_data } => error_data,
            error => {
                assert!(
                    self.unrecoverable_error.is_none(),
                    "Trying to set an unrecoverable error twice in Native Syscall Handler"
                );
                self.unrecoverable_error = Some(unwrap_native_error(error));
                *remaining_gas = 0;
                vec![]
            }
        }
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
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().deploy_gas_cost)?;

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
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().storage_read_gas_cost)?;

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
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().storage_write_gas_cost)?;

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
        to_address: Felt,
        payload: &[Felt],
        remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().send_message_to_l1_gas_cost,
        )?;

        let order = self.context.n_sent_messages_to_l1;
        let to_address = EthAddress::try_from(to_address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        self.l2_to_l1_messages.push(OrderedL2ToL1Message {
            order,
            message: MessageToL1 { to_address, payload: L2ToL1Payload(payload.to_vec()) },
        });

        self.context.n_sent_messages_to_l1 += 1;

        Ok(())
    }

    fn keccak(&mut self, input: &[u64], remaining_gas: &mut u128) -> SyscallResult<U256> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().keccak_gas_cost)?;

        const KECCAK_FULL_RATE_IN_WORDS: usize = 17;

        let input_length = input.len();
        let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

        if remainder != 0 {
            return Err(self.handle_error(
                remaining_gas,
                SyscallExecutionError::SyscallError {
                    error_data: vec![Felt::from_hex(INVALID_INPUT_LENGTH_ERROR).unwrap()],
                },
            ));
        }

        // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
        // works.
        let n_rounds_as_u128 = u128::try_from(n_rounds).expect("Failed to convert usize to u128.");
        let gas_cost =
            n_rounds_as_u128 * u128::from(self.context.gas_costs().keccak_round_cost_gas_cost);

        if gas_cost > *remaining_gas {
            return Err(self.handle_error(
                remaining_gas,
                SyscallExecutionError::SyscallError {
                    error_data: vec![Felt::from_hex(OUT_OF_GAS_ERROR).unwrap()],
                },
            ));
        }
        *remaining_gas -= gas_cost;

        let mut state = [0u64; 25];
        for chunk in input.chunks(KECCAK_FULL_RATE_IN_WORDS) {
            for (i, val) in chunk.iter().enumerate() {
                state[i] ^= val;
            }
            keccak::f1600(&mut state)
        }

        Ok(U256 {
            hi: u128::from(state[2]) | (u128::from(state[3]) << 64),
            lo: u128::from(state[0]) | (u128::from(state[1]) << 64),
        })
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
        prev_state: &mut [u32; 8],
        current_block: &[u32; 16],
        remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().sha256_process_block_gas_cost,
        )?;

        let data_as_bytes = sha2::digest::generic_array::GenericArray::from_exact_iter(
            current_block.iter().flat_map(|x| x.to_be_bytes()),
        )
        .expect(
            "u32.to_be_bytes() returns 4 bytes, and data.len() == 16. So data contains 64 bytes.",
        );

        sha2::compress256(prev_state, &[data_as_bytes]);

        Ok(())
    }
}
