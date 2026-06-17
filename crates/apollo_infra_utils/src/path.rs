use std::path::PathBuf;
use std::{env, fs};

use tracing::error;

#[cfg(test)]
#[path = "path_test.rs"]
mod path_test;

/// Returns the current crate's manifest directory (`CARGO_MANIFEST_DIR`), resolved at runtime.
///
/// Cargo and nextest set `CARGO_MANIFEST_DIR` in the environment of the test/build process they
/// spawn, so reading it at runtime yields the path of the checkout that is actually executing.
/// Falls back to the compile-time `env!` value when the variable is absent (e.g. a binary run
/// outside Cargo).
///
/// Runtime resolution is deliberate. `env!("CARGO_MANIFEST_DIR")` bakes the path in at compile
/// time, and a shared `rustc-wrapper` cache (e.g. sccache with a shared cache directory) can serve
/// an object that was compiled in a *different* checkout. In a git worktree that makes `env!`
/// return another checkout's path, so `expect_file!`/fixture writes land in the wrong repository.
/// Reading the variable at runtime always reflects the checkout that is executing.
#[macro_export]
macro_rules! cargo_manifest_dir {
    () => {
        ::std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_string())
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
    PathBuf::from(cargo_manifest_dir!()).ancestors().nth(2).expect("Cannot navigate up").into()
}

// TODO(Tsabary/ Arni): consider alternatives.
pub fn current_dir() -> std::io::Result<PathBuf> {
    std::env::current_dir()
}
