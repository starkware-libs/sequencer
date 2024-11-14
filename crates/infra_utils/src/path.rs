use std::env;
use std::path::PathBuf;

// TODO(Tsabary/ Arni): consolidate with other get_absolute_path functions.
/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    let base_dir = env::var("CARGO_MANIFEST_DIR")
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| PathBuf::from(dir).join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"));
    base_dir.join(relative_path)
}
