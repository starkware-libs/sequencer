pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";
#[cfg(feature = "cairo_native")]
pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";
#[cfg(feature = "cairo_native")]
#[allow(dead_code)] // NOTE: Only used in build.rs - therefore recognized as dead code.
pub(crate) const CAIRO_NATIVE_CRATE_PATH: &str = "crates/bin/starknet-native-compile";

pub(crate) const SHARED_EXECUTABLES_DIR: &str = "target/shared_executables";

#[cfg(feature = "cairo_native")]
pub(crate) const NATIVE_COMPILE_OUT_DIR: &str = "target/native_compile_outputs";
#[cfg(feature = "cairo_native")]
pub(crate) const COMPILED_OUTPUT_NAME: &str = "output.so";
#[cfg(feature = "cairo_native")]
#[allow(dead_code)] // NOTE: Only used in build.rs - therefore recognized as dead code.
pub(crate) const RUNTIME_LIBRARY_PATH: &str = "crates/blockifier/libcairo_native_runtime.a";
