use std::collections::HashSet;
use std::convert::From;
use std::fmt;
use std::hash::RandomState;
use std::sync::Arc;

use ark_ec::short_weierstrass::{Affine, Projective, SWCurveConfig};
use ark_ff::{BigInt, PrimeField};
use cairo_native::starknet::{
    BlockInfo,
    ExecutionInfo,
    ExecutionInfoV2,
    Secp256k1Point,
    Secp256r1Point,
    StarknetSyscallHandler,
    SyscallResult,
    TxInfo,
    TxV2Info,
    U256,
};
use num_bigint::BigUint;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{
    calculate_contract_address,
    ClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::{EventContent, EventData, EventKey, L2ToL1Payload};
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{
    CallInfo,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::contract_class::RunnableContractClass;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    ConstructorContext,
    EntryPointExecutionContext,
};
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::execution_utils::execute_deployment;
use crate::execution::native::utils::{calculate_resource_bounds, default_tx_v2_info};
use crate::execution::secp;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    INVALID_INPUT_LENGTH_ERROR,
    OUT_OF_GAS_ERROR,
};
use crate::execution::syscalls::{exceeds_event_size_limit, syscall_base};
use crate::state::state_api::State;
use crate::transaction::objects::TransactionInfo;

pub struct NativeSyscallHandler<'state> {
    // Input for execution.
    pub state: &'state mut dyn State,
    pub context: &'state mut EntryPointExecutionContext,
    pub call: CallEntryPoint,

    // Execution results.
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub inner_calls: Vec<CallInfo>,

    // Additional information gathered during execution.
    pub read_values: Vec<Felt>,
    pub accessed_keys: HashSet<StorageKey, RandomState>,
    pub read_class_hash_values: Vec<ClassHash>,
    // Accessed addresses by the `get_class_hash_at` syscall.
    pub accessed_contract_addresses: HashSet<ContractAddress>,

    // It is set if an unrecoverable error happens during syscall execution
    pub unrecoverable_error: Option<SyscallExecutionError>,
}

impl<'state> NativeSyscallHandler<'state> {
    pub fn new(
        call: CallEntryPoint,
        state: &'state mut dyn State,
        context: &'state mut EntryPointExecutionContext,
    ) -> NativeSyscallHandler<'state> {
        NativeSyscallHandler {
            state,
            call,
            context,
            events: Vec::new(),
            l2_to_l1_messages: Vec::new(),
            inner_calls: Vec::new(),
            read_values: Vec::new(),
            accessed_keys: HashSet::new(),
            read_class_hash_values: Vec::new(),
            accessed_contract_addresses: HashSet::new(),
            unrecoverable_error: None,
        }
    }

    fn execute_inner_call(
        &mut self,
        entry_point: CallEntryPoint,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Retdata> {
        let call_info = entry_point
            .execute(self.state, self.context, remaining_gas)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        let retdata = call_info.execution.retdata.clone();

        if call_info.execution.failed {
            let error = SyscallExecutionError::SyscallError { error_data: retdata.0 };
            return Err(self.handle_error(remaining_gas, error));
        }

        self.inner_calls.push(call_info);

        Ok(retdata)
    }

    /// Handles all gas-related logics and perform additional checks. In native,
    /// we need to explicitly call this method at the beginning of each syscall.
    fn pre_execute_syscall(
        &mut self,
        remaining_gas: &mut u64,
        syscall_gas_cost: u64,
    ) -> SyscallResult<()> {
        if self.unrecoverable_error.is_some() {
            // An unrecoverable error was found in a previous syscall, we return immediatly to
            // accelerate the end of the execution. The returned data is not important
            return Err(vec![]);
        }
        // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
        let required_gas = syscall_gas_cost - self.context.gas_costs().syscall_base_gas_cost;

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

    fn handle_error(&mut self, remaining_gas: &mut u64, error: SyscallExecutionError) -> Vec<Felt> {
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

    fn get_tx_info_v1(&self) -> TxInfo {
        let tx_info = &self.context.tx_context.tx_info;
        TxInfo {
            version: tx_info.version().0,
            account_contract_address: Felt::from(tx_info.sender_address()),
            max_fee: tx_info.max_fee_for_execution_info_syscall().0,
            signature: tx_info.signature().0,
            transaction_hash: tx_info.transaction_hash().0,
            chain_id: Felt::from_hex(
                &self.context.tx_context.block_context.chain_info.chain_id.as_hex(),
            )
            .expect("Failed to convert the chain_id to hex."),
            nonce: tx_info.nonce().0,
        }
    }

    fn get_block_info(&self) -> BlockInfo {
        let block_info = &self.context.tx_context.block_context.block_info;
        if self.context.execution_mode == ExecutionMode::Validate {
            let versioned_constants = self.context.versioned_constants();
            let block_number = block_info.block_number.0;
            let block_timestamp = block_info.block_timestamp.0;
            // Round down to the nearest multiple of validate_block_number_rounding.
            let validate_block_number_rounding =
                versioned_constants.get_validate_block_number_rounding();
            let rounded_block_number =
                (block_number / validate_block_number_rounding) * validate_block_number_rounding;
            // Round down to the nearest multiple of validate_timestamp_rounding.
            let validate_timestamp_rounding = versioned_constants.get_validate_timestamp_rounding();
            let rounded_timestamp =
                (block_timestamp / validate_timestamp_rounding) * validate_timestamp_rounding;
            BlockInfo {
                block_number: rounded_block_number,
                block_timestamp: rounded_timestamp,
                sequencer_address: Felt::ZERO,
            }
        } else {
            BlockInfo {
                block_number: block_info.block_number.0,
                block_timestamp: block_info.block_timestamp.0,
                sequencer_address: Felt::from(block_info.sequencer_address),
            }
        }
    }

    fn get_tx_info_v2(&self) -> SyscallResult<TxV2Info> {
        let tx_info = &self.context.tx_context.tx_info;
        let native_tx_info = TxV2Info {
            version: tx_info.version().0,
            account_contract_address: Felt::from(tx_info.sender_address()),
            max_fee: tx_info.max_fee_for_execution_info_syscall().0,
            signature: tx_info.signature().0,
            transaction_hash: tx_info.transaction_hash().0,
            chain_id: Felt::from_hex(
                &self.context.tx_context.block_context.chain_info.chain_id.as_hex(),
            )
            .expect("Failed to convert the chain_id to hex."),
            nonce: tx_info.nonce().0,
            ..default_tx_v2_info()
        };

        match tx_info {
            TransactionInfo::Deprecated(_) => Ok(native_tx_info),
            TransactionInfo::Current(context) => Ok(TxV2Info {
                resource_bounds: calculate_resource_bounds(context)?,
                tip: context.tip.0.into(),
                paymaster_data: context.paymaster_data.0.clone(),
                nonce_data_availability_mode: context.nonce_data_availability_mode.into(),
                fee_data_availability_mode: context.fee_data_availability_mode.into(),
                account_deployment_data: context.account_deployment_data.0.clone(),
                ..native_tx_info
            }),
        }
    }
}

impl<'state> StarknetSyscallHandler for &mut NativeSyscallHandler<'state> {
    fn get_block_hash(
        &mut self,
        block_number: u64,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().get_block_hash_gas_cost)?;

        match syscall_base::get_block_hash_base(self.context, block_number, self.state) {
            Ok(value) => Ok(value),
            Err(e) => Err(self.handle_error(remaining_gas, e)),
        }
    }

    fn get_execution_info(&mut self, remaining_gas: &mut u64) -> SyscallResult<ExecutionInfo> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().get_execution_info_gas_cost,
        )?;

        Ok(ExecutionInfo {
            block_info: self.get_block_info(),
            tx_info: self.get_tx_info_v1(),
            caller_address: Felt::from(self.call.caller_address),
            contract_address: Felt::from(self.call.storage_address),
            entry_point_selector: self.call.entry_point_selector.0,
        })
    }

    fn get_class_hash_at(
        &mut self,
        contract_address: Felt,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().get_class_hash_at_gas_cost,
        )?;
        let request = ContractAddress::try_from(contract_address)
            .map_err(|err| self.handle_error(remaining_gas, err.into()))?;
        self.accessed_contract_addresses.insert(request);

        let class_hash = self
            .state
            .get_class_hash_at(request)
            .map_err(|err| self.handle_error(remaining_gas, err.into()))?;
        self.read_class_hash_values.push(class_hash);

        Ok(class_hash.0)
    }

    fn get_execution_info_v2(&mut self, remaining_gas: &mut u64) -> SyscallResult<ExecutionInfoV2> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().get_execution_info_gas_cost,
        )?;

        Ok(ExecutionInfoV2 {
            block_info: self.get_block_info(),
            tx_info: self.get_tx_info_v2()?,
            caller_address: Felt::from(self.call.caller_address),
            contract_address: Felt::from(self.call.storage_address),
            entry_point_selector: self.call.entry_point_selector.0,
        })
    }

    fn deploy(
        &mut self,
        class_hash: Felt,
        contract_address_salt: Felt,
        calldata: &[Felt],
        deploy_from_zero: bool,
        remaining_gas: &mut u64,
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

        let call_info =
            execute_deployment(self.state, self.context, ctor_context, calldata, remaining_gas)
                .map_err(|err| self.handle_error(remaining_gas, err.into()))?;

        let constructor_retdata = call_info.execution.retdata.0[..].to_vec();

        self.inner_calls.push(call_info);

        Ok((Felt::from(deployed_contract_address), constructor_retdata))
    }
    fn replace_class(&mut self, class_hash: Felt, remaining_gas: &mut u64) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().replace_class_gas_cost)?;

        let class_hash = ClassHash(class_hash);
        let contract_class = self
            .state
            .get_compiled_contract_class(class_hash)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        match contract_class {
            RunnableContractClass::V0(_) => Err(self.handle_error(
                remaining_gas,
                SyscallExecutionError::ForbiddenClassReplacement { class_hash },
            )),
            RunnableContractClass::V1(_) | RunnableContractClass::V1Native(_) => {
                self.state
                    .set_class_hash_at(self.call.storage_address, class_hash)
                    .map_err(|e| self.handle_error(remaining_gas, e.into()))?;

                Ok(())
            }
        }
    }

    fn library_call(
        &mut self,
        class_hash: Felt,
        function_selector: Felt,
        calldata: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().library_call_gas_cost)?;

        let class_hash = ClassHash(class_hash);

        let wrapper_calldata = Calldata(Arc::new(calldata.to_vec()));

        let entry_point = CallEntryPoint {
            class_hash: Some(class_hash),
            code_address: None,
            entry_point_type: EntryPointType::External,
            entry_point_selector: EntryPointSelector(function_selector),
            calldata: wrapper_calldata,
            // The call context remains the same in a library call.
            storage_address: self.call.storage_address,
            caller_address: self.call.caller_address,
            call_type: CallType::Delegate,
            initial_gas: u64::try_from(*remaining_gas)
                .expect("Failed to convert gas (u128 -> u64)"),
        };

        Ok(self.execute_inner_call(entry_point, remaining_gas)?.0)
    }

    fn call_contract(
        &mut self,
        address: Felt,
        entry_point_selector: Felt,
        calldata: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().call_contract_gas_cost)?;

        let contract_address = ContractAddress::try_from(address)
            .map_err(|error| self.handle_error(remaining_gas, error.into()))?;
        if self.context.execution_mode == ExecutionMode::Validate
            && self.call.storage_address != contract_address
        {
            let err = SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "call_contract".to_string(),
                execution_mode: self.context.execution_mode,
            };
            return Err(self.handle_error(remaining_gas, err));
        }

        let wrapper_calldata = Calldata(Arc::new(calldata.to_vec()));

        let entry_point = CallEntryPoint {
            class_hash: None,
            code_address: Some(contract_address),
            entry_point_type: EntryPointType::External,
            entry_point_selector: EntryPointSelector(entry_point_selector),
            calldata: wrapper_calldata,
            storage_address: contract_address,
            caller_address: self.call.caller_address,
            call_type: CallType::Call,
            initial_gas: u64::try_from(*remaining_gas)
                .expect("Failed to convert gas from u128 to u64."),
        };

        Ok(self.execute_inner_call(entry_point, remaining_gas)?.0)
    }

    fn storage_read(
        &mut self,
        address_domain: u32,
        address: Felt,
        remaining_gas: &mut u64,
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
        remaining_gas: &mut u64,
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
        keys: &[Felt],
        data: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().emit_event_gas_cost)?;

        let order = self.context.n_emitted_events;
        let event = EventContent {
            keys: keys.iter().copied().map(EventKey).collect(),
            data: EventData(data.to_vec()),
        };

        exceeds_event_size_limit(
            self.context.versioned_constants(),
            self.context.n_emitted_events + 1,
            &event,
        )
        .map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        self.events.push(OrderedEvent { order, event });
        self.context.n_emitted_events += 1;

        Ok(())
    }

    fn send_message_to_l1(
        &mut self,
        to_address: Felt,
        payload: &[Felt],
        remaining_gas: &mut u64,
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

    fn keccak(&mut self, input: &[u64], remaining_gas: &mut u64) -> SyscallResult<U256> {
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
        let n_rounds_as_u128 = u64::try_from(n_rounds).expect("Failed to convert usize to u128.");
        let gas_cost =
            n_rounds_as_u128 * u64::from(self.context.gas_costs().keccak_round_cost_gas_cost);

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
        x: U256,
        y: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256k1Point>> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256k1_new_gas_cost)?;

        Secp256Point::new(x, y)
            .map(|op| op.map(|p| p.into()))
            .map_err(|e| self.handle_error(remaining_gas, e))
    }

    fn secp256k1_add(
        &mut self,
        p0: Secp256k1Point,
        p1: Secp256k1Point,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256k1Point> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256k1_add_gas_cost)?;

        Ok(Secp256Point::add(p0.into(), p1.into()).into())
    }

    fn secp256k1_mul(
        &mut self,
        p: Secp256k1Point,
        m: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256k1Point> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256k1_mul_gas_cost)?;

        Ok(Secp256Point::mul(p.into(), m).into())
    }

    fn secp256k1_get_point_from_x(
        &mut self,
        x: U256,
        y_parity: bool,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256k1Point>> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().secp256k1_get_point_from_x_gas_cost,
        )?;

        Secp256Point::get_point_from_x(x, y_parity)
            .map(|op| op.map(|p| p.into()))
            .map_err(|e| self.handle_error(remaining_gas, e))
    }

    fn secp256k1_get_xy(
        &mut self,
        p: Secp256k1Point,
        remaining_gas: &mut u64,
    ) -> SyscallResult<(U256, U256)> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().secp256k1_get_xy_gas_cost,
        )?;

        Ok((p.x, p.y))
    }

    fn secp256r1_new(
        &mut self,
        x: U256,
        y: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256r1Point>> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256r1_new_gas_cost)?;

        Secp256Point::new(x, y)
            .map(|option| option.map(|p| p.into()))
            .map_err(|err| self.handle_error(remaining_gas, err))
    }

    fn secp256r1_add(
        &mut self,
        p0: Secp256r1Point,
        p1: Secp256r1Point,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256r1Point> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256r1_add_gas_cost)?;
        Ok(Secp256Point::add(p0.into(), p1.into()).into())
    }

    fn secp256r1_mul(
        &mut self,
        p: Secp256r1Point,
        m: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256r1Point> {
        self.pre_execute_syscall(remaining_gas, self.context.gas_costs().secp256r1_mul_gas_cost)?;

        Ok(Secp256Point::mul(p.into(), m).into())
    }

    fn secp256r1_get_point_from_x(
        &mut self,
        x: U256,
        y_parity: bool,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256r1Point>> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().secp256r1_get_point_from_x_gas_cost,
        )?;

        Secp256Point::get_point_from_x(x, y_parity)
            .map(|option| option.map(|p| p.into()))
            .map_err(|err| self.handle_error(remaining_gas, err))
    }

    fn secp256r1_get_xy(
        &mut self,
        p: Secp256r1Point,
        remaining_gas: &mut u64,
    ) -> SyscallResult<(U256, U256)> {
        self.pre_execute_syscall(
            remaining_gas,
            self.context.gas_costs().secp256r1_get_xy_gas_cost,
        )?;

        Ok((p.x, p.y))
    }

    fn sha256_process_block(
        &mut self,
        prev_state: &mut [u32; 8],
        current_block: &[u32; 16],
        remaining_gas: &mut u64,
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

/// A wrapper around an elliptic curve point in affine coordinates (x,y) on a
/// short Weierstrass curve, specifically for Secp256k1/r1 curves.
///
/// This type provides a unified interface for working with points on both
/// secp256k1 and secp256r1 curves through the generic `Curve` parameter.
#[derive(PartialEq, Clone, Copy)]
struct Secp256Point<Curve: SWCurveConfig>(Affine<Curve>);
impl From<Secp256Point<ark_secp256k1::Config>> for Secp256k1Point {
    fn from(Secp256Point(Affine { x, y, infinity }): Secp256Point<ark_secp256k1::Config>) -> Self {
        Secp256k1Point {
            x: big4int_to_u256(x.into()),
            y: big4int_to_u256(y.into()),
            is_infinity: infinity,
        }
    }
}

impl From<Secp256Point<ark_secp256r1::Config>> for Secp256r1Point {
    fn from(Secp256Point(Affine { x, y, infinity }): Secp256Point<ark_secp256r1::Config>) -> Self {
        Secp256r1Point {
            x: big4int_to_u256(x.into()),
            y: big4int_to_u256(y.into()),
            is_infinity: infinity,
        }
    }
}

impl From<Secp256k1Point> for Secp256Point<ark_secp256k1::Config> {
    fn from(p: Secp256k1Point) -> Self {
        Secp256Point(Affine {
            x: u256_to_big4int(p.x).into(),
            y: u256_to_big4int(p.y).into(),
            infinity: p.is_infinity,
        })
    }
}

impl From<Secp256r1Point> for Secp256Point<ark_secp256r1::Config> {
    fn from(p: Secp256r1Point) -> Self {
        Secp256Point(Affine {
            x: u256_to_big4int(p.x).into(),
            y: u256_to_big4int(p.y).into(),
            infinity: p.is_infinity,
        })
    }
}

impl<Curve: SWCurveConfig> Secp256Point<Curve>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    fn wrap_secp_result<T>(
        result: Result<Option<T>, SyscallExecutionError>,
    ) -> Result<Option<Secp256Point<Curve>>, SyscallExecutionError>
    where
        T: Into<Affine<Curve>>,
    {
        match result {
            Ok(None) => Ok(None),
            Ok(Some(point)) => Ok(Some(Secp256Point(point.into()))),
            Err(error) => Err(error),
        }
    }

    /// Given an (x, y) pair, this function:
    /// - Returns the point at infinity for (0, 0).
    /// - Returns `Err` if either `x` or `y` is outside the modulus.
    /// - Returns `Ok(None)` if (x, y) are within the modulus but not on the curve.
    /// - Ok(Some(Point)) if (x,y) are on the curve.
    fn new(x: U256, y: U256) -> Result<Option<Self>, SyscallExecutionError> {
        let x = u256_to_biguint(x);
        let y = u256_to_biguint(y);

        Self::wrap_secp_result(secp::new_affine(x, y))
    }

    fn add(p0: Self, p1: Self) -> Self {
        let result: Projective<Curve> = p0.0 + p1.0;
        Secp256Point(result.into())
    }

    fn mul(p: Self, m: U256) -> Self {
        let result = p.0 * Curve::ScalarField::from(u256_to_biguint(m));
        Secp256Point(result.into())
    }

    fn get_point_from_x(x: U256, y_parity: bool) -> Result<Option<Self>, SyscallExecutionError> {
        let x = u256_to_biguint(x);

        Self::wrap_secp_result(secp::get_point_from_x(x, y_parity))
    }
}

impl<Curve: SWCurveConfig> fmt::Debug for Secp256Point<Curve> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secp256Point").field(&self.0).finish()
    }
}

fn u256_to_biguint(u256: U256) -> BigUint {
    let lo = BigUint::from(u256.lo);
    let hi = BigUint::from(u256.hi);

    (hi << 128) + lo
}

fn big4int_to_u256(b_int: BigInt<4>) -> U256 {
    let [a, b, c, d] = b_int.0;

    let lo = u128::from(a) | (u128::from(b) << 64);
    let hi = u128::from(c) | (u128::from(d) << 64);

    U256 { lo, hi }
}

fn u256_to_big4int(u256: U256) -> BigInt<4> {
    fn to_u64s(bytes: [u8; 16]) -> (u64, u64) {
        let lo_bytes: [u8; 8] = bytes[0..8].try_into().expect("Take high bytes");
        let lo: u64 = u64::from_le_bytes(lo_bytes);
        let hi_bytes: [u8; 8] = bytes[8..16].try_into().expect("Take low bytes");
        let hi: u64 = u64::from_le_bytes(hi_bytes);
        (lo, hi)
    }
    let (hi_lo, hi_hi) = to_u64s(u256.hi.to_le_bytes());
    let (lo_lo, lo_hi) = to_u64s(u256.lo.to_le_bytes());
    BigInt::new([lo_lo, lo_hi, hi_lo, hi_hi])
}

#[cfg(test)]
mod test {
    use cairo_native::starknet::U256;

    use crate::execution::native::syscall_handler::Secp256Point;

    #[test]
    fn infinity_test() {
        let p1 =
            Secp256Point::<ark_secp256k1::Config>::get_point_from_x(U256 { lo: 1, hi: 0 }, false)
                .unwrap()
                .unwrap();

        let p2 = Secp256Point::mul(p1, U256 { lo: 0, hi: 0 });
        assert!(p2.0.infinity);

        assert_eq!(p1, Secp256Point::add(p1, p2));
    }
}
