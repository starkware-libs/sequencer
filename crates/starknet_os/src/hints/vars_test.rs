use apollo_starknet_os_program::OS_PROGRAM;
use strum::IntoEnumIterator;

use crate::hints::vars::{CairoStruct, Const};

/// Test that all constants in the Const enum can be successfully fetched from the OS program.
/// This verifies that every constant defined in the enum actually exists in the compiled program.
#[test]
fn test_const_enum_constants_exist() {
    // Iterate over all constants and test that each can be fetched.
    for constant in Const::iter() {
        let full_path: &str = constant.into();
        // Take everything after the last ‘.’.
        let name = full_path.rsplit('.').next().unwrap_or(full_path);
        constant.fetch(&OS_PROGRAM.constants).unwrap_or_else(|_| {
            panic!("Failed to fetch constant `{name}` from OS program in the path {full_path}")
        });
    }
}

/// Test that all struct types in the CairoStruct enum exist in the OS program.
/// This verifies that every struct defined in the enum is actually present in the compiled program.
#[test]
fn test_cairo_struct_enum_structs_exist() {
    for cstruct in CairoStruct::iter() {
        let full_path: &str = cstruct.into();
        // If this is a pointer type (ends with '*'), drop the '*' for lookup.
        let lookup_path = full_path.strip_suffix('*').unwrap_or(full_path);
        // Take everything after the last ‘.’.
        let name = lookup_path.rsplit('.').next().unwrap_or(lookup_path);

        // look in identifiers instead of `.structs`
        OS_PROGRAM.get_identifier(lookup_path).unwrap_or_else(|| {
            panic!("Failed to fetch Cairo struct `{name}` from OS program in the path {full_path}")
        });
    }
}
