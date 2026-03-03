use toml_test_utils::{DependencyValue, ROOT_TOML};

use crate::constants::CAIRO_NATIVE_GIT_REV;

// TODO(Avi): Revert to checking version once cairo-native is pinned to a stable crates.io release.
#[test]
fn required_cairo_native_version_test() {
    let cairo_native_rev = ROOT_TOML
        .dependencies()
        .filter_map(|(name, value)| match (name.as_str(), value) {
            ("cairo-native", DependencyValue::Object { rev, .. }) => rev.as_ref(),
            _ => None,
        })
        .next()
        .expect("cairo-native dependency with rev not found in root toml file.");
    assert_eq!(CAIRO_NATIVE_GIT_REV, cairo_native_rev);
}
