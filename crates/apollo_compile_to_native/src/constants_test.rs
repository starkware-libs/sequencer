use toml_test_utils::{DependencyValue, ROOT_TOML};

use crate::constants::{CAIRO_NATIVE_GIT_REV, CAIRO_NATIVE_GIT_URL};

#[test]
fn cairo_native_git_dependency_test() {
    let (git_url, git_rev) = ROOT_TOML
        .dependencies()
        .filter_map(|(name, value)| match (name.as_str(), value) {
            ("cairo-native", DependencyValue::Object { git: Some(git), rev, .. }) => {
                Some((git.clone(), rev.clone()))
            }
            _ => None,
        })
        .next()
        .expect("cairo-native git dependency not found in root toml file.");

    assert_eq!(CAIRO_NATIVE_GIT_URL, git_url);
    assert_eq!(
        Some(CAIRO_NATIVE_GIT_REV.to_string()),
        git_rev,
        "cairo-native git revision mismatch"
    );
}
