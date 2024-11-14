use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static PATH_TO_CARGO_MANIFEST_DIR: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| env::var("CARGO_MANIFEST_DIR").ok().map(|dir| Path::new(&dir).into()));

pub fn cargo_manifest_dir() -> Option<PathBuf> {
    PATH_TO_CARGO_MANIFEST_DIR.clone()
}

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

fn path_of_project_root() -> PathBuf {
    cargo_manifest_dir()
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| dir.join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or(env::current_dir().expect("Failed to get current directory"))
}
