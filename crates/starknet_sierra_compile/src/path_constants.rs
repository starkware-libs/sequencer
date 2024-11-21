use std::path::{Path, PathBuf};

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";
#[cfg(feature = "cairo_native")]
pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

pub fn project_root_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").to_path_buf()
}

pub(crate) const SHARED_EXECUTABLES_DIR: &str = "target/shared_executables";

#[cfg(feature = "cairo_native")]
pub(crate) const NATIVE_COMPILE_OUT_DIR: &str = "target/native_compile_outputs";
#[cfg(feature = "cairo_native")]
pub(crate) const COMPILED_OUTPUT_NAME: &str = "output.so";
