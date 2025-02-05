// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";
#[cfg(feature = "cairo_native")]
pub(crate) const NATIVE_COMPILE_BINARY_NAME: &str = "starknet-native-compile";

#[cfg(feature = "cairo_native")]
pub const REQUIRED_NATIVE_COMPILE_VERSION: &str = "0.14.0-rc.0";
