use cairo_vm::types::builtin_name::BuiltinName;
use starknet_api::execution_resources::{Builtin, Opcode};
use strum::IntoEnumIterator;

use crate::execution::call_info::OpcodeName;

/// Converts OpcodeName to starknet_api::Opcode.
/// The match must be exhaustive - compiler will error if a new OpcodeName is added.
fn opcode_name_to_starknet_api(opcode: OpcodeName) -> Opcode {
    match opcode {
        OpcodeName::Blake => Opcode::Blake,
    }
}

/// Converts BuiltinName to starknet_api::Builtin.
/// The match must be exhaustive - compiler will error if a new BuiltinName is added.
fn builtin_name_to_starknet_api(builtin: BuiltinName) -> Option<Builtin> {
    match builtin {
        BuiltinName::output => None, // output has no starknet_api equivalent
        BuiltinName::range_check => Some(Builtin::RangeCheck),
        BuiltinName::pedersen => Some(Builtin::Pedersen),
        BuiltinName::ecdsa => Some(Builtin::Ecdsa),
        BuiltinName::keccak => Some(Builtin::Keccak),
        BuiltinName::bitwise => Some(Builtin::Bitwise),
        BuiltinName::ec_op => Some(Builtin::EcOp),
        BuiltinName::poseidon => Some(Builtin::Poseidon),
        BuiltinName::segment_arena => Some(Builtin::SegmentArena),
        BuiltinName::range_check96 => Some(Builtin::RangeCheck96),
        BuiltinName::add_mod => Some(Builtin::AddMod),
        BuiltinName::mul_mod => Some(Builtin::MulMod),
    }
}

/// Verifies that every OpcodeName variant has a corresponding starknet_api::Opcode.
#[test]
fn test_opcode_enums_are_synced() {
    for opcode_name in OpcodeName::iter() {
        let _ = opcode_name_to_starknet_api(opcode_name);
    }
}

/// Verifies that every BuiltinName variant has a corresponding starknet_api::Builtin mapping.
/// Note: We can't iterate over BuiltinName (no EnumIter), but the exhaustive match in
/// builtin_name_to_starknet_api ensures all variants are covered at compile time.
#[test]
fn test_builtin_enums_are_synced() {
    // The exhaustive match in builtin_name_to_starknet_api ensures coverage.
    // Test a few representative conversions to ensure the function works.
    assert_eq!(builtin_name_to_starknet_api(BuiltinName::pedersen), Some(Builtin::Pedersen));
    assert_eq!(builtin_name_to_starknet_api(BuiltinName::output), None);
}
