use std::path::{Path, PathBuf};

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";
#[cfg(feature = "cairo_native")]
pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

fn out_dir() -> PathBuf {
    Path::new(&std::env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable"))
        .to_path_buf()
}

/// Get the crate's `OUT_DIR` and navigate up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared across all crates in this project.
fn target_dir() -> PathBuf {
    let out_dir = out_dir();

    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

fn shared_folder_dir() -> PathBuf {
    target_dir().join("shared_executables")
}

pub fn binary_path(binary_name: &str) -> PathBuf {
    shared_folder_dir().join(binary_name)
}

#[cfg(feature = "cairo_native")]
pub fn output_file_path() -> String {
    out_dir().join("output.so").to_str().unwrap().into()
}
