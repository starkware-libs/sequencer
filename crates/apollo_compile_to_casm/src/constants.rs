// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";

#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_LANG_VERSION: &str = "2.11.2";
// TODO(Elin): test version alignment with Cargo.
<<<<<<< HEAD:crates/apollo_compile_to_casm/src/constants.rs
||||||| fa359cdbb:crates/apollo_sierra_multicompile/src/constants.rs
#[cfg(feature = "cairo_native")]
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.3.4";
=======
#[cfg(feature = "cairo_native")]
pub const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.4.0";
>>>>>>> origin/main:crates/apollo_sierra_multicompile/src/constants.rs
