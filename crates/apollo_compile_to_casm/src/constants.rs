// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";

// TODO(Elin): test version alignment with Cargo.
#[allow(dead_code)]
pub(crate) const REQUIRED_CAIRO_LANG_VERSION: &str = "2.12.0-dev.0";
