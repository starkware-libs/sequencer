// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

#[cfg(feature = "cairo_native")]
pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

#[cfg(feature = "cairo_native")]
#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
