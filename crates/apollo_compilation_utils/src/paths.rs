// Note: This module includes path resolution functions that are needed during build and run times.
// It must not contain functionality that is available in only in one of these modes. Specifically,
// it must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

use std::path::{Path, PathBuf};

#[cfg(test)]
#[path = "paths_test.rs"]
pub mod test;

/// Returns `<cargo_tools_root>/<binary>-<version>/bin/<binary>`, where
/// `cargo_tools_root` resolves to `$CARGO_TOOLS_ROOT`, else a `tools` directory
/// next to the running executable (if one exists), else `$CARGO_HOME/tools`,
/// else `$HOME/.cargo/tools`.
///
/// The path is constructed deterministically; there is no `$PATH` lookup. The
/// version is part of the path, so a version mismatch surfaces as "file not
/// found" at startup rather than as a silent run against the wrong binary.
pub fn binary_path(binary_name: &str, required_version: &str) -> PathBuf {
    cargo_tools_root()
        .join(format!("{binary_name}-{required_version}"))
        .join("bin")
        .join(binary_name)
}

fn cargo_tools_root() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_TOOLS_ROOT") {
        return PathBuf::from(p);
    }
    // A `tools` directory next to the running executable takes precedence over the
    // cargo-home default: distributed packages ship the compiler tree alongside the
    // binary, so prebuilt binaries work on machines with no cargo installation and
    // no environment setup.
    if let Some(tools_root) =
        std::env::current_exe().ok().and_then(|exe_path| exe_relative_tools_root(&exe_path))
    {
        return tools_root;
    }
    let cargo_home = std::env::var("CARGO_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").expect("HOME must be set")).join(".cargo")
    });
    cargo_home.join("tools")
}

/// Returns the `tools` directory next to `exe_path`, or `None` if there is no such
/// directory (e.g. for executables under `target/`, which have no bundled tools).
fn exe_relative_tools_root(exe_path: &Path) -> Option<PathBuf> {
    let tools_dir = exe_path.parent()?.join("tools");
    tools_dir.is_dir().then_some(tools_dir)
}
