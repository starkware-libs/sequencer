use std::env;
use std::path::PathBuf;

// TODO(Tsabary/ Arni): consolidate with other get_absolute_path functions.
/// Resolves a relative path from the project root directory and returns its absolute path.
///
/// # Arguments
/// * `relative_path` - A string slice representing the relative path from the project root.
///
/// # Returns
/// * An absolute `PathBuf` representing the resolved path starting from the project root.
pub fn resolve_project_relative_path(relative_path: &str) -> PathBuf {
    path_of_project_root().join(relative_path)
}

/// Resolves a relative path from the project root directory at runtime and returns its absolute
/// path.
///
/// # Arguments
/// * `relative_path` - A string slice representing the relative path from the project root.
///
/// # Returns
/// * An absolute `PathBuf` representing the resolved path starting from the project root.
pub fn runtime_resolve_project_relative_path(relative_path: &str) -> PathBuf {
    runtime_path_of_project_root().join(relative_path)
}

fn path_of_project_root() -> PathBuf {
    env::var("CARGO_MANIFEST_DIR")
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| PathBuf::from(dir).join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"))
}

fn runtime_path_of_project_root() -> PathBuf {
    env::current_exe()
        .expect("Failed to get current executable path")
        .ancestors()
        .nth(4)
        .expect("Failed to navigate up four levels from the executable")
        .to_path_buf()
}
