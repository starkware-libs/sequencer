use apollo_infra_utils::cairo_compiler_version::cairo1_compiler_version;
#[cfg(feature = "cairo_native")]
use toml_test_utils::{DependencyValue, ROOT_TOML};

use crate::constants::REQUIRED_CAIRO_LANG_VERSION;
#[cfg(feature = "cairo_native")]
use crate::constants::REQUIRED_CAIRO_NATIVE_VERSION;

#[cfg(feature = "cairo_native")]
#[test]
fn required_cairo_native_version_test() {
    let cairo_native_version = ROOT_TOML
        .dependencies()
        .filter_map(|(name, value)| match (name.as_str(), value) {
            ("cairo-native", DependencyValue::Object { version, .. }) => version.as_ref(),
            ("cairo-native", DependencyValue::String(version)) => Some(version),
            _ => None,
        })
        .next()
        .expect("cairo-native dependency not found in root toml file.");
    assert_eq!(REQUIRED_CAIRO_NATIVE_VERSION, cairo_native_version);
}

#[test]
fn cairo_compiler_version() {
    let binary_version = REQUIRED_CAIRO_LANG_VERSION;
    let cargo_version = cairo1_compiler_version();

    // Only run the assertion if version >= 2.11.
    // For older versions, just return, effectively skipping the test.
    if cargo_version.starts_with("2.11") {
        assert_eq!(
            binary_version, cargo_version,
            "Compiler version mismatch; binary version: '{}', Cargo version: '{}'.",
            binary_version, cargo_version
        );
        panic!(
            "Please remove the conditional (but leave the assertion!),
            so version alignment is always tested."
        );
    }
}
