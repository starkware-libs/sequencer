// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.
//
// The cairo-native dependency (version, git URL, branch/tag/rev) is defined only in the workspace
// Cargo.toml; the build script reads it from there to install the starknet-native-compile binary.

pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";
