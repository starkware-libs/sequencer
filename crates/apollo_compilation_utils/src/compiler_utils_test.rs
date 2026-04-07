use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use rstest::rstest;

use crate::compiler_utils::format_compiler_error;

/// Simulates a SIGKILL scenario where stderr only contains resource limit setup messages.
/// The error message should not include the filtered noise lines.
#[rstest]
fn sigkill_with_only_resource_setup_lines() {
    let stderr = "\
Setting Resource::CPU limits: soft=10, hard=10
Setting Resource::FSIZE limits: soft=26214400, hard=26214400
Setting Resource::AS limits: soft=10737418240, hard=10737418240";

    // Signal 9 = SIGKILL. On Unix, a process killed by signal N has wait status = N.
    let status = ExitStatus::from_raw(9);

    let result = format_compiler_error(stderr, &status);
    assert!(
        !result.contains("Setting Resource::"),
        "Resource setup lines should be filtered out, got: {result}"
    );
    assert!(result.contains("SIGKILL"), "SIGKILL description should be present, got: {result}");
}

/// When stderr contains a real error mixed with noise, only the meaningful error is kept.
#[rstest]
fn real_error_with_backtrace_and_resource_lines() {
    let stderr = "\
Setting Resource::CPU limits: soft=10, hard=10
error: Compilation of contract failed.
stack backtrace:
   0: std::backtrace_rs::backtrace::trace
   1: core::fmt::write";

    let status = ExitStatus::from_raw(0);

    let result = format_compiler_error(stderr, &status);
    assert_eq!(result, "error: Compilation of contract failed.");
}

/// When stderr contains only meaningful error lines, they are preserved as-is.
#[rstest]
fn meaningful_error_only() {
    let stderr = "error: Type not found.";

    let status = ExitStatus::from_raw(0);

    let result = format_compiler_error(stderr, &status);
    assert_eq!(result, "error: Type not found.");
}
