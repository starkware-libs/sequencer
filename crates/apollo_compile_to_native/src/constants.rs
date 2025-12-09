// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

pub const CAIRO_NATIVE_GIT_URL: &str = "https://github.com/lambdaclass/cairo_native";
pub const CAIRO_NATIVE_GIT_REV: &str = "941149cf65fa4a0bafeccd0bfa1c4d138c543f05";
