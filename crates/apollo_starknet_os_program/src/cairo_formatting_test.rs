use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format_batch;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;
use rstest::rstest;

use crate::CAIRO_FILES_MAP;

#[rstest]
#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    // Collect all files into a map for batch processing.
    let files: std::collections::HashMap<String, &String> =
        CAIRO_FILES_MAP.iter().map(|(path, content)| (path.to_string(), content)).collect();

    // Format all files in a single batch (much faster than per-file).
    let formatted_files = cairo0_format_batch(files);

    // Verify each formatted file matches the expected output.
    for (relative_path, formatted) in formatted_files {
        let path = apollo_starknet_os_path.join(&relative_path);
        expect_file![path].assert_eq(&formatted);
    }
}
