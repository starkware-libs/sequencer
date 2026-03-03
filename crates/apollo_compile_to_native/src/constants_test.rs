use toml_test_utils::{DependencyValue, ROOT_TOML};

/// Ensures the workspace defines cairo-native and that a required-version identifier can be
/// derived (from version string, or from git branch/tag/rev in the real build script).
#[test]
fn workspace_cairo_native_dependency_test() {
    assert!(
        ROOT_TOML.contains_dependency("cairo-native"),
        "workspace.dependencies must define cairo-native (single source of truth for install)"
    );
    let (name, value) = ROOT_TOML
        .dependencies()
        .find(|(n, _)| n.as_str() == "cairo-native")
        .expect("cairo-native not found in workspace.dependencies");
    assert_eq!(name, "cairo-native");
    match value {
        DependencyValue::String(version) => {
            assert!(
                !version.trim_start_matches('=').is_empty(),
                "cairo-native version string must be non-empty"
            );
        }
        DependencyValue::Object { version, .. } => {
            // Git deps (branch/tag/rev) have no version field; build script derives
            // required_version from branch/tag/rev. Here we only check the dependency
            // is present and valid.
            if let Some(v) = version {
                assert!(
                    !v.trim_start_matches('=').is_empty(),
                    "cairo-native version must be non-empty when set"
                );
            }
        }
    }
}
