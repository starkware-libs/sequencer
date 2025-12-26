use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use expect_test::expect_file;

use crate::CAIRO_FILES_MAP;

#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    // Read all cairo files recursively from the directory.
    for (relative_path, contents) in CAIRO_FILES_MAP.iter() {
        let formatted = cairo0_format(contents);
        expect_file![apollo_starknet_os_path.join(relative_path)].assert_eq(&formatted);
    }
}
