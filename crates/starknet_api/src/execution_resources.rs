use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use crate::block::GasPrice;
use crate::transaction::Fee;

#[derive(
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sum,
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
)]
pub struct GasAmount(pub u64);

impl GasAmount {
    pub const MAX: Self = Self(u64::MAX);
}

impl From<GasAmount> for Felt {
    fn from(gas_amount: GasAmount) -> Self {
        Self::from(gas_amount.0)
    }
}

#[derive(
    derive_more::Add,
    derive_more::Sum,
    derive_more::AddAssign,
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Deserialize,
    Serialize,
)]
pub struct GasVector {
    pub l1_gas: GasAmount,
    pub l1_data_gas: GasAmount,
    #[serde(default)]
    pub l2_gas: GasAmount,
}

impl GasVector {
    pub fn from_l1_gas(l1_gas: GasAmount) -> Self {
        Self { l1_gas, ..Default::default() }
    }

    pub fn from_l1_data_gas(l1_data_gas: GasAmount) -> Self {
        Self { l1_data_gas, ..Default::default() }
    }

    pub fn from_l2_gas(l2_gas: GasAmount) -> Self {
        Self { l2_gas, ..Default::default() }
    }

    /// Computes the cost (in fee token units) of the gas vector (saturating on overflow).
    pub fn saturated_cost(&self, gas_price: GasPrice, blob_gas_price: GasPrice) -> Fee {
        let l1_gas_cost = self
            .l1_gas
            .checked_mul(gas_price)
            .unwrap_or_else(|| {
                log::warn!(
                    "L1 gas cost overflowed: multiplication of {:?} by {:?} resulted in overflow.",
                    self.l1_gas,
                    gas_price
                );
                Fee(u128::MAX)
            })
            .0;
        let l1_data_gas_cost = self
            .l1_data_gas
            .checked_mul(blob_gas_price)
            .unwrap_or_else(|| {
                log::warn!(
                    "L1 blob gas cost overflowed: multiplication of {:?} by {:?} resulted in \
                     overflow.",
                    self.l1_data_gas,
                    blob_gas_price
                );
                Fee(u128::MAX)
            })
            .0;
        let total = l1_gas_cost.checked_add(l1_data_gas_cost).unwrap_or_else(|| {
            log::warn!(
                "Total gas cost overflowed: addition of {} and {} resulted in overflow.",
                l1_gas_cost,
                l1_data_gas_cost
            );
            u128::MAX
        });
        Fee(total)
    }
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
