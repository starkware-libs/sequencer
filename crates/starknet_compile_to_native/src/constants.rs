// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_NATIVE_BINARY_NAME: &str = "starknet-native-compile";

<<<<<<<< HEAD:crates/apollo_sierra_multicompile/src/constants.rs
pub const REQUIRED_CAIRO_LANG_VERSION: &str = "2.11.2";
// TODO(Elin): test version alignment with Cargo.
#[cfg(feature = "cairo_native")]
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
|||||||| 05c74b1e9:crates/starknet_sierra_multicompile/src/constants.rs
#[cfg(feature = "cairo_native")]
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
========
#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
>>>>>>>> origin/main-v0.13.5:crates/starknet_compile_to_native/src/constants.rs
