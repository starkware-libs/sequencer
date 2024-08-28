use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct GasVector {
    pub l1_gas: u64,
    pub l1_data_gas: u64,
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
