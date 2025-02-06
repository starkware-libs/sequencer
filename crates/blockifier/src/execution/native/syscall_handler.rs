use std::convert::From;
use std::fmt;
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
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::{EventContent, EventData, EventKey, L2ToL1Payload};
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::GasCosts;
use crate::execution::call_info::{MessageToL1, Retdata};
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    EntryPointExecutionContext,
    ExecutableCallEntryPoint,
};
use crate::execution::errors::EntryPointExecutionError;
use crate::execution::native::utils::{calculate_resource_bounds, default_tx_v2_info};
use crate::execution::secp;
use crate::execution::syscalls::hint_processor::{SyscallExecutionError, OUT_OF_GAS_ERROR};
use crate::execution::syscalls::syscall_base::SyscallHandlerBase;
use crate::state::state_api::State;
use crate::transaction::objects::TransactionInfo;

pub const CALL_CONTRACT_SELECTOR_NAME: &str = "call_contract";
pub const LIBRARY_CALL_SELECTOR_NAME: &str = "library_call";
pub struct NativeSyscallHandler<'state> {
    pub base: Box<SyscallHandlerBase<'state>>,

    // It is set if an unrecoverable error happens during syscall execution
    pub unrecoverable_error: Option<SyscallExecutionError>,
}

impl<'state> NativeSyscallHandler<'state> {
    pub fn new(
        call: ExecutableCallEntryPoint,
        state: &'state mut dyn State,
        context: &'state mut EntryPointExecutionContext,
    ) -> NativeSyscallHandler<'state> {
        NativeSyscallHandler {
            base: Box::new(SyscallHandlerBase::new(call, state, context)),
            unrecoverable_error: None,
        }
    }

    pub fn gas_costs(&self) -> &GasCosts {
        self.base.context.gas_costs()
    }

    /// Handles all gas-related logics and perform additional checks. In native,
    /// we need to explicitly call this method at the beginning of each syscall.
    fn pre_execute_syscall(
        &mut self,
        remaining_gas: &mut u64,
        syscall_gas_cost: u64,
    ) -> SyscallResult<()> {
        if self.unrecoverable_error.is_some() {
            // An unrecoverable error was found in a previous syscall, we return immediately to
            // accelerate the end of the execution. The returned data is not important
            return Err(vec![]);
        }
        // Refund `SYSCALL_BASE_GAS_COST` as it was pre-charged.
        let required_gas = syscall_gas_cost - self.gas_costs().base.syscall_base_gas_cost;

        if *remaining_gas < required_gas {
            // Out of gas failure.
            return Err(vec![
                Felt::from_hex(OUT_OF_GAS_ERROR)
                    .expect("Failed to parse OUT_OF_GAS_ERROR hex string"),
            ]);
        }

        *remaining_gas -= required_gas;

        // To support sierra gas charge for blockifier revert flow, we track the remaining gas left
        // before executing a syscall if the current tracked resource is gas.
        // 1. If the syscall does not run Cairo code (i.e. not library call, not call contract, and
        //    not a deploy), any failure will not run in the OS, so no need to charge - the value
        //    before entering the callback is good enough to charge.
        // 2. If the syscall runs Cairo code, but the tracked resource is steps (and not gas), the
        //    additional charge of reverted cairo steps will cover the inner cost, and the outer
        //    cost we track here will be the additional reverted gas.
        // 3. If the syscall runs Cairo code and the tracked resource is gas, either the inner
        //    failure will be a Cairo1 revert (and the gas consumed on the call info will override
        //    the current tracked value), or we will pass through another syscall before failing -
        //    and by induction (we will reach this point again), the gas will be charged correctly.
        self.base.context.update_revert_gas_with_next_remaining_gas(GasAmount(*remaining_gas));

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
            SyscallExecutionError::Revert { error_data } => error_data,
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

    fn execute_inner_call(
        &mut self,
        entry_point: CallEntryPoint,
        remaining_gas: &mut u64,
        class_hash: ClassHash,
        error_wrapper_fn: impl Fn(
            SyscallExecutionError,
            ClassHash,
            ContractAddress,
            EntryPointSelector,
        ) -> SyscallExecutionError,
    ) -> SyscallResult<Retdata> {
        let entry_point_clone = entry_point.clone();
        let raw_data = self.base.execute_inner_call(entry_point, remaining_gas).map_err(|e| {
            self.handle_error(
                remaining_gas,
                match e {
                    SyscallExecutionError::Revert { .. } => e,
                    _ => error_wrapper_fn(
                        e,
                        class_hash,
                        entry_point_clone.storage_address,
                        entry_point_clone.entry_point_selector,
                    ),
                },
            )
        })?;
        Ok(Retdata(raw_data))
    }

    fn get_tx_info_v1(&self) -> TxInfo {
        let tx_info = &self.base.context.tx_context.tx_info;
        TxInfo {
            version: self.base.tx_version_for_get_execution_info().0,
            account_contract_address: Felt::from(tx_info.sender_address()),
            max_fee: tx_info.max_fee_for_execution_info_syscall().0,
            signature: tx_info.signature().0,
            transaction_hash: tx_info.transaction_hash().0,
            chain_id: Felt::from_hex(
                &self.base.context.tx_context.block_context.chain_info.chain_id.as_hex(),
            )
            .expect("Failed to convert the chain_id to hex."),
            nonce: tx_info.nonce().0,
        }
    }

    fn get_block_info(&self) -> BlockInfo {
        let block_info = match self.base.context.execution_mode {
            ExecutionMode::Execute => self.base.context.tx_context.block_context.block_info(),
            ExecutionMode::Validate => {
                &self.base.context.tx_context.block_context.block_info_for_validate()
            }
        };
        BlockInfo {
            block_number: block_info.block_number.0,
            block_timestamp: block_info.block_timestamp.0,
            sequencer_address: Felt::from(block_info.sequencer_address),
        }
    }

    fn get_tx_info_v2(&self) -> SyscallResult<TxV2Info> {
        let tx_info = &self.base.context.tx_context.tx_info;
        let native_tx_info = TxV2Info {
            version: self.base.tx_version_for_get_execution_info().0,
            account_contract_address: Felt::from(tx_info.sender_address()),
            max_fee: tx_info.max_fee_for_execution_info_syscall().0,
            signature: tx_info.signature().0,
            transaction_hash: tx_info.transaction_hash().0,
            chain_id: Felt::from_hex(
                &self.base.context.tx_context.block_context.chain_info.chain_id.as_hex(),
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
    pub fn finalize(&mut self) {
        self.base.finalize();
    }
}

impl StarknetSyscallHandler for &mut NativeSyscallHandler<'_> {
    fn get_block_hash(
        &mut self,
        block_number: u64,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.get_block_hash)?;

        match self.base.get_block_hash(block_number) {
            Ok(value) => Ok(value),
            Err(e) => Err(self.handle_error(remaining_gas, e)),
        }
    }

    fn get_execution_info(&mut self, remaining_gas: &mut u64) -> SyscallResult<ExecutionInfo> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.get_execution_info)?;

        Ok(ExecutionInfo {
            block_info: self.get_block_info(),
            tx_info: self.get_tx_info_v1(),
            caller_address: Felt::from(self.base.call.caller_address),
            contract_address: Felt::from(self.base.call.storage_address),
            entry_point_selector: self.base.call.entry_point_selector.0,
        })
    }

    fn get_class_hash_at(
        &mut self,
        contract_address: Felt,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.get_class_hash_at)?;
        let request = ContractAddress::try_from(contract_address)
            .map_err(|err| self.handle_error(remaining_gas, err.into()))?;

        let class_hash = self
            .base
            .get_class_hash_at(request)
            .map_err(|err| self.handle_error(remaining_gas, err))?;
        Ok(class_hash.0)
    }

    fn get_execution_info_v2(&mut self, remaining_gas: &mut u64) -> SyscallResult<ExecutionInfoV2> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.get_execution_info)?;

        Ok(ExecutionInfoV2 {
            block_info: self.get_block_info(),
            tx_info: self.get_tx_info_v2()?,
            caller_address: Felt::from(self.base.call.caller_address),
            contract_address: Felt::from(self.base.call.storage_address),
            entry_point_selector: self.base.call.entry_point_selector.0,
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
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.deploy)?;

        let (deployed_contract_address, call_info) = self
            .base
            .deploy(
                ClassHash(class_hash),
                ContractAddressSalt(contract_address_salt),
                Calldata(Arc::new(calldata.to_vec())),
                deploy_from_zero,
                remaining_gas,
            )
            .map_err(|err| self.handle_error(remaining_gas, err))?;

        let constructor_retdata = call_info.execution.retdata.0[..].to_vec();
        self.base.inner_calls.push(call_info);

        Ok((Felt::from(deployed_contract_address), constructor_retdata))
    }
    fn replace_class(&mut self, class_hash: Felt, remaining_gas: &mut u64) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.replace_class)?;

        self.base
            .replace_class(ClassHash(class_hash))
            .map_err(|err| self.handle_error(remaining_gas, err))?;
        Ok(())
    }

    fn library_call(
        &mut self,
        class_hash: Felt,
        function_selector: Felt,
        calldata: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.library_call)?;

        let class_hash = ClassHash(class_hash);

        let wrapper_calldata = Calldata(Arc::new(calldata.to_vec()));

        let selector = EntryPointSelector(function_selector);

        let entry_point = CallEntryPoint {
            class_hash: Some(class_hash),
            code_address: None,
            entry_point_type: EntryPointType::External,
            entry_point_selector: selector,
            calldata: wrapper_calldata,
            // The call context remains the same in a library call.
            storage_address: self.base.call.storage_address,
            caller_address: self.base.call.caller_address,
            call_type: CallType::Delegate,
            initial_gas: *remaining_gas,
        };

        let error_wrapper_function =
            |e: SyscallExecutionError,
             class_hash: ClassHash,
             storage_address: ContractAddress,
             selector: EntryPointSelector| {
                e.as_lib_call_execution_error(class_hash, storage_address, selector)
            };

        Ok(self
            .execute_inner_call(entry_point, remaining_gas, class_hash, error_wrapper_function)?
            .0)
    }

    fn call_contract(
        &mut self,
        address: Felt,
        entry_point_selector: Felt,
        calldata: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<Vec<Felt>> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.call_contract)?;

        let contract_address = ContractAddress::try_from(address)
            .map_err(|error| self.handle_error(remaining_gas, error.into()))?;

        let class_hash = self
            .base
            .state
            .get_class_hash_at(contract_address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        if self.base.context.execution_mode == ExecutionMode::Validate
            && self.base.call.storage_address != contract_address
        {
            let err = SyscallExecutionError::InvalidSyscallInExecutionMode {
                syscall_name: "call_contract".to_string(),
                execution_mode: self.base.context.execution_mode,
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
            caller_address: self.base.call.storage_address,
            call_type: CallType::Call,
            initial_gas: *remaining_gas,
        };

        let error_wrapper_function =
            |e: SyscallExecutionError,
             class_hash: ClassHash,
             storage_address: ContractAddress,
             selector: EntryPointSelector| {
                e.as_call_contract_execution_error(class_hash, storage_address, selector)
            };

        Ok(self
            .execute_inner_call(entry_point, remaining_gas, class_hash, error_wrapper_function)?
            .0)
    }

    fn storage_read(
        &mut self,
        address_domain: u32,
        address: Felt,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Felt> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.storage_read)?;

        if address_domain != 0 {
            let address_domain = Felt::from(address_domain);
            let error = SyscallExecutionError::InvalidAddressDomain { address_domain };
            return Err(self.handle_error(remaining_gas, error));
        }

        let key = StorageKey::try_from(address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;

        let value = self.base.storage_read(key).map_err(|e| self.handle_error(remaining_gas, e))?;
        Ok(value)
    }

    fn storage_write(
        &mut self,
        address_domain: u32,
        address: Felt,
        value: Felt,
        remaining_gas: &mut u64,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.storage_write)?;

        if address_domain != 0 {
            let address_domain = Felt::from(address_domain);
            let error = SyscallExecutionError::InvalidAddressDomain { address_domain };
            return Err(self.handle_error(remaining_gas, error));
        }

        let key = StorageKey::try_from(address)
            .map_err(|e| self.handle_error(remaining_gas, e.into()))?;
        self.base.storage_write(key, value).map_err(|e| self.handle_error(remaining_gas, e))?;

        Ok(())
    }

    fn emit_event(
        &mut self,
        keys: &[Felt],
        data: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.emit_event)?;

        let event = EventContent {
            keys: keys.iter().copied().map(EventKey).collect(),
            data: EventData(data.to_vec()),
        };

        self.base.emit_event(event).map_err(|e| self.handle_error(remaining_gas, e))?;
        Ok(())
    }

    fn send_message_to_l1(
        &mut self,
        to_address: Felt,
        payload: &[Felt],
        remaining_gas: &mut u64,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.send_message_to_l1)?;

        let to_address = EthAddress::try_from(to_address)
            .map_err(|err| self.handle_error(remaining_gas, err.into()))?;
        let message = MessageToL1 { to_address, payload: L2ToL1Payload(payload.to_vec()) };

        self.base.send_message_to_l1(message).map_err(|err| self.handle_error(remaining_gas, err))
    }

    fn keccak(&mut self, input: &[u64], remaining_gas: &mut u64) -> SyscallResult<U256> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.keccak)?;

        match self.base.keccak(input, remaining_gas) {
            Ok((state, _n_rounds)) => Ok(U256 {
                hi: u128::from(state[2]) | (u128::from(state[3]) << 64),
                lo: u128::from(state[0]) | (u128::from(state[1]) << 64),
            }),
            Err(err) => Err(self.handle_error(remaining_gas, err)),
        }
    }

    fn secp256k1_new(
        &mut self,
        x: U256,
        y: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256k1Point>> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256k1_new)?;

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
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256k1_add)?;

        Ok(Secp256Point::add(p0.into(), p1.into()).into())
    }

    fn secp256k1_mul(
        &mut self,
        p: Secp256k1Point,
        m: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256k1Point> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256k1_mul)?;

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
            self.gas_costs().syscalls.secp256k1_get_point_from_x,
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
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256k1_get_xy)?;

        Ok((p.x, p.y))
    }

    fn secp256r1_new(
        &mut self,
        x: U256,
        y: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Option<Secp256r1Point>> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256r1_new)?;

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
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256r1_add)?;
        Ok(Secp256Point::add(p0.into(), p1.into()).into())
    }

    fn secp256r1_mul(
        &mut self,
        p: Secp256r1Point,
        m: U256,
        remaining_gas: &mut u64,
    ) -> SyscallResult<Secp256r1Point> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256r1_mul)?;

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
            self.gas_costs().syscalls.secp256r1_get_point_from_x,
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
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.secp256r1_get_xy)?;

        Ok((p.x, p.y))
    }

    fn sha256_process_block(
        &mut self,
        prev_state: &mut [u32; 8],
        current_block: &[u32; 16],
        remaining_gas: &mut u64,
    ) -> SyscallResult<()> {
        self.pre_execute_syscall(remaining_gas, self.gas_costs().syscalls.sha256_process_block)?;

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
