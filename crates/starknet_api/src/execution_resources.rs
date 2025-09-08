use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sizeof::SizeOf;
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use crate::block::{GasPrice, GasPriceVector, NonzeroGasPrice};
use crate::transaction::fields::{Fee, Resource, Tip};

#[cfg_attr(
    any(test, feature = "testing"),
    derive(
        derive_more::Sum,
        derive_more::Div,
        derive_more::SubAssign,
        derive_more::Sub,
        derive_more::Add,
        derive_more::AddAssign,
    )
)]
#[derive(
    derive_more::Display,
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Hash,
    SizeOf,
)]
pub struct GasAmount(pub u64);

impl From<GasAmount> for Felt {
    fn from(gas_amount: GasAmount) -> Self {
        Self::from(gas_amount.0)
    }
}

macro_rules! impl_from_uint_for_gas_amount {
    ($($uint:ty),*) => {
        $(
            impl From<$uint> for GasAmount {
                fn from(value: $uint) -> Self {
                    Self(u64::from(value))
                }
            }
        )*
    };
}

impl_from_uint_for_gas_amount!(u8, u16, u32, u64);

impl GasAmount {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u64::MAX);

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }

    pub const fn nonzero_saturating_mul(self, rhs: NonzeroGasPrice) -> Fee {
        rhs.saturating_mul(self)
    }

    pub const fn saturating_mul(self, rhs: GasPrice) -> Fee {
        rhs.saturating_mul(self)
    }

    pub fn checked_mul(self, rhs: GasPrice) -> Option<Fee> {
        rhs.checked_mul(self)
    }

    pub fn checked_factor_mul(self, factor: u64) -> Option<Self> {
        self.0.checked_mul(factor).map(Self)
    }

    pub fn checked_factor_div(self, factor: u64) -> Option<Self> {
        self.0.checked_div(factor).map(Self)
    }
}

#[cfg_attr(
    any(test, feature = "testing"),
    derive(derive_more::Add, derive_more::Sum, derive_more::AddAssign)
)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct GasVector {
    pub l1_gas: GasAmount,
    pub l1_data_gas: GasAmount,
    #[serde(default)]
    pub l2_gas: GasAmount,
}

impl GasVector {
    pub const ZERO: GasVector =
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(0), l2_gas: GasAmount(0) };

    pub fn from_l1_gas(l1_gas: GasAmount) -> Self {
        Self { l1_gas, ..Default::default() }
    }

    pub fn from_l1_data_gas(l1_data_gas: GasAmount) -> Self {
        Self { l1_data_gas, ..Default::default() }
    }

    pub fn from_l2_gas(l2_gas: GasAmount) -> Self {
        Self { l2_gas, ..Default::default() }
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        match (
            self.l1_gas.checked_add(rhs.l1_gas),
            self.l1_data_gas.checked_add(rhs.l1_data_gas),
            self.l2_gas.checked_add(rhs.l2_gas),
        ) {
            (Some(l1_gas), Some(l1_data_gas), Some(l2_gas)) => {
                Some(Self { l1_gas, l1_data_gas, l2_gas })
            }
            _ => None,
        }
    }

    pub fn checked_scalar_mul(self, factor: u64) -> Option<Self> {
        match (
            self.l1_gas.checked_factor_mul(factor),
            self.l1_data_gas.checked_factor_mul(factor),
            self.l2_gas.checked_factor_mul(factor),
        ) {
            (Some(l1_gas), Some(l1_data_gas), Some(l2_gas)) => {
                Some(Self { l1_gas, l1_data_gas, l2_gas })
            }
            _ => None,
        }
    }

    /// Computes the cost (in fee token units) of the gas vector (panicking on overflow).
    pub fn cost(&self, gas_prices: &GasPriceVector, tip: Tip) -> Fee {
        let tipped_l2_gas_price =
            gas_prices.l2_gas_price.checked_add(tip.into()).unwrap_or_else(|| {
                panic!(
                    "Tip overflowed: addition of L2 gas price ({}) and tip ({}) resulted in \
                     overflow.",
                    gas_prices.l2_gas_price, tip
                )
            });

        let mut sum = Fee(0);
        for (gas, price, resource) in [
            (self.l1_gas, gas_prices.l1_gas_price, Resource::L1Gas),
            (self.l1_data_gas, gas_prices.l1_data_gas_price, Resource::L1DataGas),
            (self.l2_gas, tipped_l2_gas_price, Resource::L2Gas),
        ] {
            let cost = gas.checked_mul(price.get()).unwrap_or_else(|| {
                panic!(
                    "{} cost overflowed: multiplication of gas amount ({}) by price per unit ({}) \
                     resulted in overflow.",
                    resource, gas, price
                )
            });
            sum = sum.checked_add(cost).unwrap_or_else(|| {
                panic!(
                    "Total cost overflowed: addition of current sum ({}) and cost of {} ({}) \
                     resulted in overflow.",
                    sum, resource, cost
                )
            });
        }
        sum
    }
}

/// Computes the total L1 gas amount estimation from the different given L1 gas arguments using the
/// following formula:
/// One byte of data costs either 1 data gas (in blob mode) or 16 gas (in calldata
/// mode). For gas price GP and data gas price DGP, the discount for using blobs
/// would be DGP / (16 * GP).
/// X non-data-related gas consumption and Y bytes of data, in non-blob mode, would
/// cost (X + 16*Y) units of gas. Applying the discount ratio to the data-related
/// summand, we get total_gas = (X + Y * DGP / GP).
/// If this function is called with kzg_flag==false, then l1_data_gas==0, and this discount
/// function does nothing.
/// Panics on overflow.
/// Does not take L2 gas into account - more context is required to convert L2 gas units to L1 gas
/// units (context defined outside of the current crate).
pub fn to_discounted_l1_gas(
    l1_gas_price: NonzeroGasPrice,
    l1_data_gas_price: GasPrice,
    l1_gas: GasAmount,
    l1_data_gas: GasAmount,
) -> GasAmount {
    let l1_data_gas_fee = l1_data_gas.checked_mul(l1_data_gas_price).unwrap_or_else(|| {
        panic!(
            "Discounted L1 gas cost overflowed: multiplication of L1 data gas ({}) by L1 data gas \
             price ({}) resulted in overflow.",
            l1_data_gas, l1_data_gas_price
        );
    });
    let l1_data_gas_in_l1_gas_units =
        l1_data_gas_fee.checked_div_ceil(l1_gas_price).unwrap_or_else(|| {
            panic!(
                "Discounted L1 gas cost overflowed: division of L1 data fee ({}) by regular L1 \
                 gas price ({}) resulted in overflow.",
                l1_data_gas_fee, l1_gas_price
            );
        });
    l1_gas.checked_add(l1_data_gas_in_l1_gas_units).unwrap_or_else(|| {
        panic!(
            "Overflow while computing discounted L1 gas: L1 gas ({}) + L1 data gas in L1 gas \
             units ({}) resulted in overflow.",
            l1_gas, l1_data_gas_in_l1_gas_units
        )
    })
}

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ExecutionResources {
    pub steps: u64,
    pub builtin_instance_counter: HashMap<Builtin, u64>,
    pub memory_holes: u64,
    pub da_gas_consumed: GasVector,
    pub gas_consumed: GasVector,
}

#[derive(Clone, Debug, Deserialize, EnumIter, Eq, Hash, PartialEq, Serialize)]
pub enum Builtin {
    #[serde(rename = "range_check_builtin_applications")]
    RangeCheck,
    #[serde(rename = "pedersen_builtin_applications")]
    Pedersen,
    #[serde(rename = "poseidon_builtin_applications")]
    Poseidon,
    #[serde(rename = "ec_op_builtin_applications")]
    EcOp,
    #[serde(rename = "ecdsa_builtin_applications")]
    Ecdsa,
    #[serde(rename = "bitwise_builtin_applications")]
    Bitwise,
    #[serde(rename = "keccak_builtin_applications")]
    Keccak,
    #[serde(rename = "segment_arena_builtin")]
    SegmentArena,
    #[serde(rename = "add_mod_builtin")]
    AddMod,
    #[serde(rename = "mul_mod_builtin")]
    MulMod,
    #[serde(rename = "range_check96_builtin")]
    RangeCheck96,
}

const RANGE_CHACK_BUILTIN_NAME: &str = "range_check";
const PEDERSEN_BUILTIN_NAME: &str = "pedersen";
const POSEIDON_BUILTIN_NAME: &str = "poseidon";
const EC_OP_BUILTIN_NAME: &str = "ec_op";
const ECDSA_BUILTIN_NAME: &str = "ecdsa";
const BITWISE_BUILTIN_NAME: &str = "bitwise";
const KECCAK_BUILTIN_NAME: &str = "keccak";
const SEGMENT_ARENA_BUILTIN_NAME: &str = "segment_arena";
const ADD_MOD_BUILTIN_NAME: &str = "add_mod";
const MUL_MOD_BUILTIN_NAME: &str = "mul_mod";
const RANGE_CHECK96_BUILTIN_NAME: &str = "range_check96";

impl Builtin {
    pub fn name(&self) -> &'static str {
        match self {
            Builtin::RangeCheck => RANGE_CHACK_BUILTIN_NAME,
            Builtin::Pedersen => PEDERSEN_BUILTIN_NAME,
            Builtin::Poseidon => POSEIDON_BUILTIN_NAME,
            Builtin::EcOp => EC_OP_BUILTIN_NAME,
            Builtin::Ecdsa => ECDSA_BUILTIN_NAME,
            Builtin::Bitwise => BITWISE_BUILTIN_NAME,
            Builtin::Keccak => KECCAK_BUILTIN_NAME,
            Builtin::SegmentArena => SEGMENT_ARENA_BUILTIN_NAME,
            Builtin::AddMod => ADD_MOD_BUILTIN_NAME,
            Builtin::MulMod => MUL_MOD_BUILTIN_NAME,
            Builtin::RangeCheck96 => RANGE_CHECK96_BUILTIN_NAME,
        }
    }
}
