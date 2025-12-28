use std::collections::HashMap;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};

use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Returns all cairo file paths in the given directory, recursively.
pub fn get_cairo_file_paths(dir_path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir_path)
        .unwrap()
        .flat_map(|entry| get_cairo_file_paths_recursive(entry.unwrap()))
        .collect()
}

/// Utility function to recursively find all cairo files.
fn get_cairo_file_paths_recursive(entry: DirEntry) -> Vec<PathBuf> {
    let file_type = entry.file_type().unwrap();
    let path = entry.path();
    if file_type.is_dir() {
        std::fs::read_dir(path)
            .unwrap()
            .flat_map(|entry| get_cairo_file_paths_recursive(entry.unwrap()))
            .collect()
    } else {
        assert!(file_type.is_file());
        if path.extension().unwrap_or_default() == "cairo" {
            vec![path]
        } else {
            vec![]
        }
    }
}

/// Find all files with a .cairo extension in the `src` directory, insert them into a map and dump
/// the map as JSON to the specified location.
pub fn dump_source_files(dump_to: &PathBuf) {
    println!("cargo::warning=Dumping OS source files...");

    // Recursively fetch all cairo files and contents, and convert the paths to relative paths.
    let base_path = PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");
    let map_without_prefixes: HashMap<String, String> = get_cairo_file_paths(&base_path)
        .into_iter()
        .map(|path| {
            let relative = path.strip_prefix(&base_path).unwrap().to_str().unwrap().to_string();
            let contents = std::fs::read_to_string(&path).unwrap();
            (relative, contents)
        })
        .collect();

    // Serialize and dump the map to the specified location.
    let serialized = serde_json::to_string(&map_without_prefixes).unwrap();
    std::fs::write(dump_to, serialized)
        .unwrap_or_else(|error| panic!("Failed to write to {dump_to:?}: {error:?}."));
}
