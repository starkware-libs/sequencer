use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler_test_utils::cairo0_format_batch;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use rstest::rstest;

use crate::{CAIRO_FILES_MAP, CONSTANTS_CAIRO_PATH};

#[rstest]
#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    // Collect all files into a map for batch processing.
    // Skip constants.cairo — it is auto-generated and validated by test_os_constants, which
    // also formats it. Including it here would cause a race condition between the two tests.
    let files: std::collections::HashMap<String, &String> = CAIRO_FILES_MAP
        .iter()
        .filter(|(path, _)| path.as_str() != CONSTANTS_CAIRO_PATH)
        .map(|(path, content)| (path.to_string(), content))
        .collect();

    // Format all files in a single batch (much faster than per-file).
    let formatted_files = cairo0_format_batch(files);
    // Sanity check.
    let formatted_keys: std::collections::BTreeSet<&str> =
        formatted_files.keys().map(String::as_str).collect();
    let expected_keys: std::collections::BTreeSet<&str> = CAIRO_FILES_MAP
        .keys()
        .filter(|path| path.as_str() != CONSTANTS_CAIRO_PATH)
        .map(String::as_str)
        .collect();
    assert_eq!(formatted_keys, expected_keys);

    // Verify each formatted file matches the expected output.
    for (relative_path, formatted) in formatted_files {
        let path = apollo_starknet_os_path.join(&relative_path);
        expect_file![path].assert_eq(&formatted);
    }
}
