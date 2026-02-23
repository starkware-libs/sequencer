// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

// Cairo-native is installed from a git branch (not crates.io).
pub const CAIRO_NATIVE_GIT_URL: &str = "https://github.com/lambdaclass/cairo_native";
pub const CAIRO_NATIVE_GIT_BRANCH: &str = "tomer/blake_builtin";

// Kept for the version check on the installed binary (--version output).
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.9.0-rc.1";
