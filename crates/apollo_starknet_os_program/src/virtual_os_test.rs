use expect_test::{expect, Expect};

use crate::{OS_PROGRAM, VIRTUAL_OS_PROGRAM, VIRTUAL_OS_SWAPPED_FILES};

static EXPECTED_SWAPPED_FILES: Expect = expect!["starkware/starknet/core/os/os_utils.cairo"];

static EXPECTED_OS_PROGRAM_BYTECODE_LENGTH: Expect = expect![[r#"
    15527
"#]];
static EXPECTED_VIRTUAL_OS_PROGRAM_BYTECODE_LENGTH: Expect = expect![[r#"
    13172
"#]];

/// Asserts the list of swapped virtual OS files matches the expected list.
#[test]
fn test_virtual_os_swapped_files() {
    EXPECTED_SWAPPED_FILES.assert_eq(&VIRTUAL_OS_SWAPPED_FILES.join("\n"));
}

/// Asserts the bytecode length of the OS program and virtual OS program match expected values.
/// This test helps monitor optimizations made to the virtual OS program.
#[test]
fn test_program_bytecode_lengths() {
    EXPECTED_OS_PROGRAM_BYTECODE_LENGTH.assert_debug_eq(&OS_PROGRAM.data_len());
    EXPECTED_VIRTUAL_OS_PROGRAM_BYTECODE_LENGTH.assert_debug_eq(&VIRTUAL_OS_PROGRAM.data_len());
}
