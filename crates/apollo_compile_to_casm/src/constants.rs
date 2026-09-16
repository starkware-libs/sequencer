// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

pub(crate) const CAIRO_LANG_BINARY_NAME: &str = "starknet-sierra-compile";

/// The custom libfunc list this node ships; selected by `AllowedLibfuncsList::Bundled`.
/// Copied into the runtime image at this same path.
pub(crate) const BUNDLED_ALLOWED_LIBFUNCS_PATH: &str =
    "crates/apollo_compile_to_casm/resources/allowed_libfuncs.json";
