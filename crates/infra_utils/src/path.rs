use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathResolutionError {
    // TODO(Arni): Handle manifest dir not exist here?
    #[error("No file exists at '{path}'")]
    PathDoesNotExist { path: PathBuf },
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

// TODO(tsabary): wrap path-related env::* invocations in the repo as utility functions
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
pub fn resolve_project_relative_path(relative_path: &str) -> Result<PathBuf, PathResolutionError> {
    let base_dir = path_of_project_root();

    let path = base_dir.join(relative_path);
    if !path.try_exists()? {
        return Err(PathResolutionError::PathDoesNotExist { path });
    }

    Ok(path)
}

fn path_of_project_root() -> PathBuf {
    cargo_manifest_dir()
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| dir.join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or(env::current_dir().expect("Failed to get current directory"))
}
