use std::collections::HashSet;
use std::convert::From;
use std::fmt;
use std::hash::RandomState;

use ark_ec::short_weierstrass::{Affine, Projective, SWCurveConfig};
use ark_ff::{PrimeField, Zero};
use cairo_native::starknet::{
    ExecutionInfo,
    ExecutionInfoV2,
    Secp256k1Point,
    Secp256r1Point,
    StarknetSyscallHandler,
    SyscallResult,
    U256,
};
use cairo_native::starknet_stub::u256_to_biguint;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::execution::call_info::{CallInfo, OrderedEvent, OrderedL2ToL1Message, Retdata};
use crate::execution::entry_point::{CallEntryPoint, EntryPointExecutionContext};
use crate::execution::native::utils::encode_str_as_felts;
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
        _prev_state: &mut [u32; 8],
        _current_block: &[u32; 16],
        _remaining_gas: &mut u128,
    ) -> SyscallResult<()> {
        todo!("Implement sha256_process_block syscall.");
    }
}

// todo(xrvdg) remove dead_code annotation after adding syscalls
#[allow(dead_code)]
impl<Curve: SWCurveConfig> Secp256Point<Curve>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    /// Given a (x,y) pair it will
    /// - return the point at infinity for (0,0)
    /// - Err if either x or y is outside of the modulus
    /// - Ok(None) if (x,y) are within the modules but not on the curve
    /// - Ok(Some(Point)) if (x,y) are on the curve
    fn new(x: U256, y: U256) -> Result<Option<Self>, Vec<Felt>> {
        let x = u256_to_biguint(x);
        let y = u256_to_biguint(y);

        match new_affine(x, y) {
            Ok(None) => Ok(None),
            Ok(Some(affine)) => Ok(Some(Secp256Point(affine))),
            Err(error) => Err(encode_str_as_felts(&error.to_string())),
        }
    }

    fn add(p0: Self, p1: Self) -> Self {
        let result: Projective<Curve> = p0.0 + p1.0;
        Secp256Point(result.into())
    }

    fn mul(p: Self, m: U256) -> Self {
        let result = p.0 * Curve::ScalarField::from(u256_to_biguint(m));
        Secp256Point(result.into())
    }

    fn get_point_from_x(x: U256, y_parity: bool) -> Result<Option<Self>, Vec<Felt>> {
        let x = u256_to_biguint(x);

        match get_point_from_x(x, y_parity) {
            Ok(None) => Ok(None),
            Ok(Some(point)) => Ok(Some(Secp256Point(point))),
            Err(error) => Err(encode_str_as_felts(&error.to_string())),
        }
    }
}

fn get_point_from_x<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y_parity: bool,
) -> Result<Option<Affine<Curve>>, SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    let _ = modulus_bound_check::<Curve>(&[&x])?;

    let x = x.into();
    let maybe_ec_point = Affine::<Curve>::get_ys_from_x_unchecked(x)
        .map(|(smaller, greater)| {
            // Return the correct y coordinate based on the parity.
            if ark_ff::BigInteger::is_odd(&smaller.into_bigint()) == y_parity {
                smaller
            } else {
                greater
            }
        })
        .map(|y| Affine::<Curve>::new_unchecked(x, y))
        .filter(|p| p.is_in_correct_subgroup_assuming_on_curve());

    Ok(maybe_ec_point)
}

pub fn new_affine<Curve: SWCurveConfig>(
    x: num_bigint::BigUint,
    y: num_bigint::BigUint,
) -> Result<Option<Affine<Curve>>, SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    let _ = modulus_bound_check::<Curve>(&[&x, &y])?;

    Ok(maybe_affine(x.into(), y.into()))
}

fn modulus_bound_check<Curve: SWCurveConfig>(
    bounds: &[&num_bigint::BigUint],
) -> Result<(), SyscallExecutionError>
where
    Curve::BaseField: PrimeField, // constraint for get_point_by_id
{
    let modulos = Curve::BaseField::MODULUS.into();

    if bounds.iter().any(|p| **p >= modulos) {
        let error =
            match Felt::from_hex(crate::execution::syscalls::hint_processor::INVALID_ARGUMENT) {
                Ok(err) => SyscallExecutionError::SyscallError { error_data: vec![err] },
                Err(err) => SyscallExecutionError::from(err),
            };

        return Err(error);
    }

    Ok(())
}

/// Variation on [`Affine<Curve>::new`] that doesn't panic and maps (x,y) = (0,0) -> infinity
fn maybe_affine<Curve: SWCurveConfig>(
    x: Curve::BaseField,
    y: Curve::BaseField,
) -> Option<Affine<Curve>> {
    let ec_point = if x.is_zero() && y.is_zero() {
        Affine::<Curve>::identity()
    } else {
        Affine::<Curve>::new_unchecked(x, y)
    };

    if ec_point.is_on_curve() && ec_point.is_in_correct_subgroup_assuming_on_curve() {
        Some(ec_point)
    } else {
        None
    }
}

/// Data structure to tie together k1 and r1 points to it's corresponding
/// `Affine<Curve>`
#[derive(PartialEq, Clone, Copy)]
struct Secp256Point<Curve: SWCurveConfig>(Affine<Curve>);

impl<Curve: SWCurveConfig> fmt::Debug for Secp256Point<Curve> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secp256Point").field(&self.0).finish()
    }
}

#[cfg(test)]
mod test {
    use cairo_native::starknet::U256;

    use super::Secp256Point;

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
