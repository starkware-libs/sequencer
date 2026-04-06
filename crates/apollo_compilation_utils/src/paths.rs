use std::path::PathBuf;

/// Returns the binary name as a PathBuf. `Command::new` will find it in PATH.
pub fn binary_path(binary_name: &str) -> PathBuf {
    PathBuf::from(binary_name)
}
