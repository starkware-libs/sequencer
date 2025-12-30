use expect_test::{expect, Expect};

use crate::VIRTUAL_OS_SWAPPED_FILES;

static EXPECTED_SWAPPED_FILES: Expect = expect!["starkware/starknet/core/os/os_utils.cairo"];

/// Asserts the list of swapped virtual OS files matches the expected list.
#[test]
fn test_virtual_os_swapped_files() {
    EXPECTED_SWAPPED_FILES.assert_eq(&VIRTUAL_OS_SWAPPED_FILES.join("\n"));
}
