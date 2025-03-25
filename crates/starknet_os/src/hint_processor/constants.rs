use std::collections::HashMap;
use std::sync::LazyLock;

use cairo_vm::types::builtin_name::BuiltinName;

pub(crate) static BUILTIN_INSTANCE_SIZES: LazyLock<HashMap<BuiltinName, usize>> =
    LazyLock::new(|| {
        HashMap::from([
            (BuiltinName::pedersen, 3),
            (BuiltinName::range_check, 1),
            (BuiltinName::ecdsa, 2),
            (BuiltinName::bitwise, 5),
            (BuiltinName::ec_op, 7),
            (BuiltinName::poseidon, 6),
            (BuiltinName::segment_arena, 3),
            (BuiltinName::range_check96, 1),
            (BuiltinName::add_mod, 7),
            (BuiltinName::mul_mod, 7),
            (BuiltinName::keccak, 16),
        ])
    });
