// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

<<<<<<< HEAD:crates/apollo_compile_to_native/src/constants.rs
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.5.0-rc.3";
||||||| e417a9e7d:crates/starknet_compile_to_native/src/constants.rs
#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
=======
#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.6";
>>>>>>> origin/main-v0.13.6:crates/starknet_compile_to_native/src/constants.rs
