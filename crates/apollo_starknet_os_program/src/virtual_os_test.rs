use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;

use crate::VIRTUAL_OS_SWAPPED_FILES;

static SWAPPED_FILES_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/virtual_os_swapped_files.txt")
});

/// Asserts the list of swapped virtual OS files matches the expected list.
#[test]
fn test_virtual_os_swapped_files() {
    // Add trailing newline to match files saved with trailing newlines (POSIX convention).
    let mut swapped_files_list = VIRTUAL_OS_SWAPPED_FILES.join("\n");
    if !swapped_files_list.is_empty() {
        swapped_files_list.push('\n');
    }
    expect_file![SWAPPED_FILES_PATH.as_path()].assert_eq(&swapped_files_list);
}

