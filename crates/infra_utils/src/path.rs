use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{env, fs};

#[cfg(test)]
#[path = "path_test.rs"]
mod path_test;

// TODO(tsabary): wrap path-related env::* invocations in the repo as utility functions
static PATH_TO_CARGO_MANIFEST_DIR: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| env::var("CARGO_MANIFEST_DIR").ok().map(|dir| Path::new(&dir).into()));

// TODO(Tsabary): should not be public. Use a getter instead.
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
/// * A `PathBuf` representing the resolved path starting from the project root.
pub fn resolve_project_relative_path(relative_path: &str) -> Result<PathBuf, std::io::Error> {
    let base_dir = path_of_project_root();
    let path = base_dir.join(relative_path);

    Ok(path)
}

/// Returns the absolute path of the project root directory.
///
/// # Returns
/// * A `PathBuf` representing the path of the project root.
pub fn project_path() -> Result<PathBuf, std::io::Error> {
    resolve_project_relative_path(".")
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
    let path = cargo_manifest_dir()
        // Attempt to get the `CARGO_MANIFEST_DIR` environment variable and convert it to `PathBuf`.
        // Ascend two directories ("../..") to get to the project root.
        .map(|dir| dir.join("../.."))
        // If `CARGO_MANIFEST_DIR` isn't set, fall back to the current working directory
        .unwrap_or(env::current_dir().expect("Failed to get current directory"));
    fs::canonicalize(path).expect("Failed to resolve project root path")
}

fn runtime_path_of_project_root() -> PathBuf {
    // Assumes the executable is located in target/[debug|release]/build/<build_revision>
    // FIXME: executable can also be located in target/<target-triple>/[debug|release]/... when
    // building for a specific target architecture (e.g. x86_64-unknown-linux-gnu).
    env::current_exe()
        .expect("Failed to get current executable path")
        .ancestors()
        // Ascend four directories from the executable to get to the project root.
        .nth(4)
        .expect("Failed to navigate up four levels from the executable")
        .to_path_buf()
}
