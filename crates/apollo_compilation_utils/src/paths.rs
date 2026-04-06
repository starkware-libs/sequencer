// Note: This module includes path resolution functions that are needed during build and run times.
// It must not contain functionality that is available in only in one of these modes. Specifically,
// it must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

use std::path::PathBuf;

fn target_dir(out_dir: &std::path::Path) -> std::path::PathBuf {
    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

pub fn shared_folder_dir(out_dir: &std::path::Path) -> std::path::PathBuf {
    target_dir(out_dir).join("shared_executables")
}

/// Returns `<cargo_tools_root>/<binary>-<version>/bin/<binary>`, where
/// `cargo_tools_root` resolves to `$CARGO_TOOLS_ROOT`, else `$CARGO_HOME/tools`,
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
    let cargo_home = std::env::var("CARGO_HOME").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(std::env::var("HOME").expect("HOME must be set")).join(".cargo")
    });
    cargo_home.join("tools")
}

// TODO(Avi): Remove once build.rs callers are gone.
pub fn legacy_binary_path(out_dir: &std::path::Path, binary_name: &str) -> std::path::PathBuf {
    shared_folder_dir(out_dir).join(binary_name)
}
