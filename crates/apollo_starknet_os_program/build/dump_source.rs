use std::collections::HashMap;
use std::fs::DirEntry;
use std::path::PathBuf;

use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Utility function to recursively find all cairo files.
fn get_cairo_file_map_recursive(entry: DirEntry) -> HashMap<String, String> {
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
            HashMap::from_iter(std::iter::once((
                path.to_str().unwrap().to_string(),
                std::fs::read_to_string(path).unwrap(),
            )))
        } else {
            HashMap::new()
        }
    }
}

/// Find all files with a .cairo extension in the `src` directory, insert them into a map and dump
/// the map as JSON to the specified location.
pub fn dump_source_files(dump_to: &PathBuf) {
    println!("cargo::warning=Dumping OS source files...");

    // Recursively fetch all cairo files and contents, and convert the paths to relative paths.
    let base_path = PathBuf::from(compile_time_cargo_manifest_dir!()).join("src");
    let base_path_string = base_path.to_str().unwrap();
    let map_without_prefixes: HashMap<String, String> = std::fs::read_dir(base_path_string)
        .unwrap()
        .flat_map(|entry| get_cairo_file_map_recursive(entry.unwrap()))
        .map(|(path, contents)| {
            assert!(path.starts_with(base_path_string));
            let path = path
                .strip_prefix(base_path_string)
                .unwrap()
                .strip_prefix("/cairo/")
                .unwrap()
                .to_string();
            (path, contents)
        })
        .collect();

    // Serialize and dump the map to the specified location.
    let serialized = serde_json::to_string(&map_without_prefixes).unwrap();
    std::fs::write(dump_to, serialized)
        .unwrap_or_else(|error| panic!("Failed to write to {dump_to:?}: {error:?}."));
}
