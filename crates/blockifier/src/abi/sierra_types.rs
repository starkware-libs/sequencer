use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::errors::memory_errors::MemoryError;
use cairo_vm::vm::vm_core::VirtualMachine;
use num_bigint::{BigUint, ToBigUint};
use starknet_api::core::{felt_to_u128, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_api::StarknetApiError;
use thiserror::Error;

use crate::state::errors::StateError;
use crate::state::state_api::StateReader;

pub type SierraTypeResult<T> = Result<T, SierraTypeError>;

#[derive(Debug, Error)]
pub enum SierraTypeError {
    #[error(transparent)]
    MemoryError(#[from] MemoryError),
    #[error(transparent)]
    MathError(#[from] MathError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
}

pub trait SierraType: Sized {
    fn from_memory(vm: &VirtualMachine, ptr: &mut Relocatable) -> SierraTypeResult<Self>;

    fn from_storage(
        state: &dyn StateReader,
        contract_address: &ContractAddress,
        key: &StorageKey,
    ) -> SierraTypeResult<Self>;
}

// Implementations.

// We implement the trait SierraType for SierraU128 and not for u128 since it's not guaranteed that
// we will always have only one sierra u128 type. For example, we might have two different fields
// where in one of them each cell is at most 127 bits.
pub struct SierraU128 {
    pub val: u128,
}

impl SierraU128 {
    pub fn as_value(&self) -> u128 {
        self.val
    }
}

impl SierraType for SierraU128 {
    fn from_memory(vm: &VirtualMachine, ptr: &mut Relocatable) -> SierraTypeResult<Self> {
        let felt = vm.get_integer(*ptr)?;
        *ptr = (*ptr + 1)?;
        Ok(Self { val: felt_to_u128(&felt)? })
    }

    fn from_storage(
        state: &dyn StateReader,
        contract_address: &ContractAddress,
        key: &StorageKey,
    ) -> SierraTypeResult<Self> {
        let felt = state.get_storage_at(*contract_address, *key)?;
        Ok(Self { val: felt_to_u128(&felt)? })
    }
}

pub struct SierraU256 {
    pub low_val: u128,
    pub high_val: u128,
}

impl SierraU256 {
    // TODO(barak, 01/10/2023): Move to_biguint() to the trait and call it as_value(). Use generics
    // to determine the return value.
    pub fn to_biguint(&self) -> BigUint {
        let u128_to_biguint =
            |val: u128| val.to_biguint().expect("u128 to BigUint conversion shouldn't fail.");
        (u128_to_biguint(self.high_val) << 128) + u128_to_biguint(self.low_val)
    }
}

impl SierraType for SierraU256 {
    fn from_memory(vm: &VirtualMachine, ptr: &mut Relocatable) -> Result<Self, SierraTypeError> {
        Ok(Self {
            low_val: SierraU128::from_memory(vm, ptr)?.as_value(),
            high_val: SierraU128::from_memory(vm, ptr)?.as_value(),
        })
    }

    fn from_storage(
        state: &dyn StateReader,
        contract_address: &ContractAddress,
        key: &StorageKey,
    ) -> SierraTypeResult<Self> {
        let low_val = SierraU128::from_storage(state, contract_address, key)?;
        let high_key = key.next_storage_key()?;
        let high_val = SierraU128::from_storage(state, contract_address, &high_key)?;
        Ok(Self { low_val: low_val.as_value(), high_val: high_val.as_value() })
    }
}
