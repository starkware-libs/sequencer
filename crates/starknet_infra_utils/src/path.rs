use std::path::PathBuf;
use std::{env, fs};

use tracing::error;

#[cfg(test)]
#[path = "path_test.rs"]
mod path_test;

// TODO(Tsabary): find a stable way to get access to the current crate directory at compile time.
#[macro_export]
macro_rules! compile_time_cargo_manifest_dir {
    () => {
        env!("CARGO_MANIFEST_DIR")
    };
}

/// Resolves a relative path from the project root directory and returns its absolute path.
///
/// # Arguments
/// * `relative_path` - A string slice representing the relative path from the project root.
///
/// # Returns
/// * A `PathBuf` representing the resolved path starting from the project root.
pub fn resolve_project_relative_path(relative_path: &str) -> Result<PathBuf, std::io::Error> {
    let project_root_path = path_of_project_root();
    let path = project_root_path.join(relative_path);
    let absolute_path = fs::canonicalize(path).inspect_err(|err| {
        error!(
            "Error: {:?}, project root path {:?}, relative path {:?}",
            err, project_root_path, relative_path
        );
    })?;

    Ok(absolute_path)
}

/// Returns the absolute path of the project root directory.
///
/// # Returns
/// * A `PathBuf` representing the path of the project root.
pub fn project_path() -> Result<PathBuf, std::io::Error> {
    resolve_project_relative_path(".")
}

fn path_of_project_root() -> PathBuf {
    // Ascend two directories to get to the project root. This assumes that the project root is two
    // directories above the current file.
    PathBuf::from(compile_time_cargo_manifest_dir!())
        .ancestors()
        .nth(2)
        .expect("Cannot navigate up")
        .into()
}

// TODO(Tsabary/ Arni): consider alternatives.
pub fn current_dir() -> std::io::Result<PathBuf> {
    std::env::current_dir()
}
