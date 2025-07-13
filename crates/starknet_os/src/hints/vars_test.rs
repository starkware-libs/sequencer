use apollo_starknet_os_program::OS_PROGRAM;
use strum::IntoEnumIterator;

use crate::hints::vars::Const;

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
