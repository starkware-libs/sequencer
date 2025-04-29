use std::collections::HashSet;
use std::fs::{DirEntry, File};
use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;

static CAIRO_FILE_LIST_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo_files_list.json")
});
static CAIRO_FILE_LIST: LazyLock<Vec<String>> = LazyLock::new(|| {
    serde_json::from_reader(
        File::open(&*CAIRO_FILE_LIST_PATH)
            .unwrap_or_else(|error| panic!("Failed to open {CAIRO_FILE_LIST_PATH:?}: {error:?}.")),
    )
    .unwrap_or_else(|error| panic!("Failed to deserialize {CAIRO_FILE_LIST_PATH:?}: {error:?}."))
});

/// Utility function for `get_cairo_file_list`.
fn get_cairo_file_list_recursive(entry: DirEntry) -> Vec<String> {
    let file_type = entry.file_type().unwrap();
    if file_type.is_dir() {
        std::fs::read_dir(entry.path())
            .unwrap()
            .flat_map(|entry| get_cairo_file_list_recursive(entry.unwrap()))
            .collect()
    } else {
        assert!(file_type.is_file());
        if entry.path().extension().unwrap_or_default() == "cairo" {
            Vec::from_iter(std::iter::once(entry.path().to_str().unwrap().to_string()))
        } else {
            Vec::new()
        }
    }
}

/// Find all files with a .cairo extension in the `src` directory.
fn get_cairo_file_set() -> HashSet<String> {
    let base_path = PathBuf::from(compile_time_cargo_manifest_dir!()).join("src");
    let base_path_string = base_path.to_str().unwrap();
    std::fs::read_dir(base_path_string)
        .unwrap()
        .flat_map(|entry| get_cairo_file_list_recursive(entry.unwrap()))
        .map(|path| {
            assert!(path.starts_with(base_path_string));
            path.strip_prefix(base_path_string)
                .unwrap()
                .strip_prefix("/cairo/")
                .unwrap()
                .to_string()
        })
        .collect()
}

/// Tests that the list of Cairo files in the `src` directory matches the actual set of cairo files
/// in the crate.
/// To fix this test, run the following command:
/// ```bash
/// FIX_OS_FILE_LIST=1 cargo test -p apollo_starknet_os_program test_cairo_file_list
/// ```
#[test]
fn test_cairo_file_list() {
    let actual_files = get_cairo_file_set();
    let expected_files: HashSet<String> = HashSet::from_iter(CAIRO_FILE_LIST.iter().cloned());
    if std::env::var("FIX_OS_FILE_LIST").is_ok() {
        let mut actual_files_vec = Vec::from_iter(actual_files.iter());
        actual_files_vec.sort();
        std::fs::write(
            CAIRO_FILE_LIST_PATH.as_path(),
            serde_json::to_string_pretty(&actual_files_vec).unwrap(),
        )
        .expect("Failed to write the cairo file list.");
    } else {
        assert_eq!(actual_files, expected_files);
    }
}
