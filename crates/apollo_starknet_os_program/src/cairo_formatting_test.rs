use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::cairo0_compiler_test_utils::cairo0_format_batch;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use rstest::rstest;

use crate::{CAIRO_FILES_MAP, CONSTANTS_CAIRO_PATH};

/// Cairo files to format, excluding auto-generated files.
/// Skip constants.cairo — it is auto-generated and validated by test_os_constants, which
/// also formats it. Including it here would cause a race condition between the two tests.
static CAIRO_FILES_TO_FORMAT: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    CAIRO_FILES_MAP
        .iter()
        .filter(|(path, _)| path.as_str() != CONSTANTS_CAIRO_PATH)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
});

#[rstest]
#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    // Format all files in a single batch (much faster than per-file).
    let formatted_files = cairo0_format_batch(CAIRO_FILES_TO_FORMAT.clone());
    // Sanity check.
    assert_eq!(
        formatted_files.keys().collect::<HashSet<_>>(),
        CAIRO_FILES_TO_FORMAT.keys().collect::<HashSet<_>>(),
    );

    // Verify each formatted file matches the expected output.
    for (relative_path, formatted) in formatted_files {
        let path = apollo_starknet_os_path.join(&relative_path);
        expect_file![path].assert_eq(&formatted);
    }
}
