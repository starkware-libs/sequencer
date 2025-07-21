use std::fs;
use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format;
use apollo_infra_utils::compile_time_cargo_manifest_dir;

use crate::CAIRO_FILES_MAP;

#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory.
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    let fix = std::env::var("FIX_CAIRO_FORMATTING").is_ok();
    // Read all cairo files recursively from the directory.
    for (relative_path, contents) in CAIRO_FILES_MAP.iter() {
        let formatted = cairo0_format(contents);
        if fix {
            // If FIX_CAIRO_FORMAT is set, overwrite the original file with the formatted content.
            let full_path = apollo_starknet_os_path.join(relative_path);
            fs::write(full_path, formatted).unwrap();
        } else {
            assert_eq!(
                formatted, *contents,
                "Cairo file formatting mismatch in '{relative_path}'.\nTo automatically fix \
                 formatting, run the test with the environment variable \
                 FIX_CAIRO_FORMATTING=1:\n\n    FIX_CAIRO_FORMATTING=1 cargo test -p \
                 apollo_starknet_os_program test_cairo0_formatting\n"
            );
        }
    }
}
