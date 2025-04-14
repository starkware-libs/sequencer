use apollo_infra_utils::cairo_compiler_version::cairo1_compiler_version;

use crate::constants::REQUIRED_CAIRO_LANG_VERSION;

#[test]
fn cairo_compiler_version() {
    let binary_version = REQUIRED_CAIRO_LANG_VERSION;
    let cargo_version = cairo1_compiler_version();
    assert_eq!(
        binary_version, cargo_version,
        "Compiler version mismatch; binary version: '{}', Cargo version: '{}'.",
        binary_version, cargo_version
    );
}
