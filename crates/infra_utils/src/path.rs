use std::env;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetPathError {
    // TODO(Arni): Handle manifest dir not exist here?
    #[error("No file exists at '{path}'")]
    PathDoesNotExist { path: PathBuf },
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

pub static PATH_TO_CARGO_MANIFEST_DIR: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| env::var("CARGO_MANIFEST_DIR").ok().map(|dir| Path::new(&dir).into()));

/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> Result<PathBuf, GetPathError> {
    let base_dir = PATH_TO_CARGO_MANIFEST_DIR.clone()
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| dir.join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or(env::current_dir().expect("Failed to get current directory"));
    let path_buf = base_dir.join(relative_path);
    if !path_buf.try_exists()? {
        return Err(GetPathError::PathDoesNotExist { path: path_buf });
    }

    Ok(path_buf)
}
