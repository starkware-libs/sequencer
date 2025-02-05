#![cfg(feature = "cairo_native")]
use toml_test_utils::{DependencyValue, ROOT_TOML};

use crate::constants::REQUIRED_CAIRO_NATIVE_VERSION;

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
