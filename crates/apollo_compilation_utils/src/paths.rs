// Note: This module includes path resolution functions that are needed during build and run times.
// It must not contain functionality that is available in only in one of these modes. Specifically,
// it must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

use std::path::PathBuf;

/// Returns the absolute path to `binary_name`, resolved through `$PATH` at call time.
///
/// Resolving the absolute path here (rather than relying on `Command::new(binary_name)`
/// to re-resolve each call) means the binary identity is locked in at startup. After
/// the process starts, no later `$PATH` mutation (env-var injection, dropped binary
/// earlier in `$PATH`) can redirect compiler invocations. Panics if the binary is not
/// on `$PATH`, which is the correct behavior: refusing to start is safer than starting
/// and silently using the wrong binary at the first compilation request.
pub fn binary_path(binary_name: &str) -> PathBuf {
    resolve_on_path(binary_name).unwrap_or_else(|| {
        panic!(
            "{binary_name} not found on PATH. Run 'scripts/install_compiler_binaries.sh' to \
             install it."
        )
    })
}

fn resolve_on_path(binary_name: &str) -> Option<PathBuf> {
    // Reject path-segment-containing names; the caller passes just a basename.
    if binary_name.is_empty() || binary_name.contains(std::path::MAIN_SEPARATOR) {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(binary_name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}
