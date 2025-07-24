use std::collections::HashMap;
use std::fs::{self, DirEntry};
use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::cairo0_format;
use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Utility function to recursively find all cairo files.
pub(crate) fn get_cairo_file_map_recursive(entry: DirEntry) -> HashMap<String, String> {
    let file_type = entry.file_type().unwrap();
    let path = entry.path();
    if file_type.is_dir() {
        std::fs::read_dir(path)
            .unwrap()
            .flat_map(|entry| get_cairo_file_map_recursive(entry.unwrap()).into_iter())
            .collect()
    } else {
        assert!(file_type.is_file());
        if path.extension().unwrap_or_default() == "cairo" {
            HashMap::from([(
                path.to_str().unwrap().to_string(),
                std::fs::read_to_string(path).unwrap(),
            )])
        } else {
            HashMap::new()
        }
    }
}

#[test]
fn test_cairo0_formatting() {
    // Get the path to the apollo_starknet_os directory
    let apollo_starknet_os_path =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");

    let fix = std::env::var("FIX_CAIRO_FORMATTING").is_ok();
    // Read all cairo files recursively from the directory
    for (path, contents) in fs::read_dir(&apollo_starknet_os_path)
        .unwrap()
        .flat_map(|entry| get_cairo_file_map_recursive(entry.unwrap()).into_iter())
    {
        let formatted = cairo0_format(&contents);
        if fix {
            // If FIX_CAIRO_FORMAT is set, overwrite the original file with the formatted content
            fs::write(path, formatted).unwrap();
        } else {
            assert!(
                formatted == contents,
                "Cairo file formatting mismatch in '{path}'.\nTo automatically fix formatting, \
                 run the test with the environment variable FIX_CAIRO_FORMATTING=1:\n\n    \
                 FIX_CAIRO_FORMATTING=1 cargo test -p apollo_starknet_os_program \
                 test_cairo0_formatting\n"
            );
        }
    }
}
