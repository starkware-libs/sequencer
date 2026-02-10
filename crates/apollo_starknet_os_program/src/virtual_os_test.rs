use expect_test::expect;

use crate::{OS_PROGRAM, VIRTUAL_OS_PROGRAM, VIRTUAL_OS_SWAPPED_FILES};

/// Asserts the list of swapped virtual OS files matches the expected list.
#[test]
fn test_virtual_os_swapped_files() {
    expect![[r#"
        starkware/starknet/core/os/execution/entry_point_utils.cairo
        starkware/starknet/core/os/execution/execute_syscalls.cairo
        starkware/starknet/core/os/execution/execute_transactions_inner.cairo
        starkware/starknet/core/os/execution/execution_constraints.cairo
        starkware/starknet/core/os/os_utils.cairo"#]]
    .assert_eq(&VIRTUAL_OS_SWAPPED_FILES.join("\n"));
}

/// Asserts the bytecode length of the OS program and virtual OS program match expected values.
/// This test helps monitor optimizations made to the virtual OS program.
#[test]
fn test_program_bytecode_lengths() {
    expect![[r#"
        15608
    "#]]
    .assert_debug_eq(&OS_PROGRAM.data_len());
    expect![[r#"
        10212
    "#]]
    .assert_debug_eq(&VIRTUAL_OS_PROGRAM.data_len());
}
