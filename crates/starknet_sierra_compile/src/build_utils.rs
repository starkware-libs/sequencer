use std::path::{Path, PathBuf};

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";
#[cfg(feature = "cairo_native")]
pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

pub fn project_root_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").to_path_buf()
}

/// Get the crate's `OUT_DIR` and navigate up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared across all crates in this project.
fn target_dir() -> PathBuf {
    project_root_path().join("target")
}

fn shared_folder_dir() -> PathBuf {
    target_dir().join("shared_executables")
}

pub fn binary_path(binary_name: &str) -> PathBuf {
    shared_folder_dir().join(binary_name)
}

#[cfg(feature = "cairo_native")]
pub fn output_file_path() -> String {
    target_dir().join("tmp/native_compile_output.so").to_str().unwrap().into()
}
