use std::path::PathBuf;

use toml_test_utils::{DependencyValue, PackageEntryValue, MEMBER_TOMLS};

/// Hard-coded list of crates that are allowed to use test code in their (non-dev) dependencies.
/// Should only contain test-related crates.
static CRATES_ALLOWED_TO_USE_TESTING_FEATURE: [&str; 6] = [
    "apollo_integration_tests",
    "apollo_test_utils",
    "blockifier_test_utils",
    "papyrus_load_test",
    "transaction_generator",
    // The CLI crate exposes tests that require test utils in dependencies.
    // TODO(Dori): Consider splitting the build of the CLI crate to a test build and a production
    //   build.
    "starknet_committer_and_os_cli",
];

/// Tests that no member crate has itself in it's dependency tree.
/// This may occur if, for example, a developer wants to activate a feature of a crate in tests, and
/// adds a dependency on itself in dev-dependencies with this feature active.
/// Note: a common (erroneous) use case would be to activate the "testing" feature of a crate in
/// tests by adding a dependency on itself in dev-dependencies. This is not allowed; any code gated
/// by the testing feature should also be gated by `test`.
#[test]
fn test_no_self_dependencies() {
    let members_with_self_deps: Vec<String> = MEMBER_TOMLS
        .iter()
        .filter_map(|(name, toml)| {
            if toml.member_dependency_names_recursive(true).contains(toml.package_name()) {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(
        members_with_self_deps.is_empty(),
        "The following crates have themselves in their dependency tree: \
         {members_with_self_deps:?}. This is not allowed."
    );
}

#[test]
fn test_package_names_match_directory() {
    let mismatched_packages: Vec<_> = MEMBER_TOMLS
        .iter()
        .filter_map(|(path_str, toml)| {
            let path = PathBuf::from(&path_str);
            let directory_name = path.file_name()?.to_str()?;
            match toml.package.get("name") {
                Some(PackageEntryValue::String(package_name)) if package_name == directory_name => {
                    None
                }
                _ => Some(path_str),
            }
        })
        .collect();
    assert!(
        mismatched_packages.is_empty(),
        "The following crates have package names that do not match their directory names, or are \
         missing a name field: {mismatched_packages:?}."
    );
}

/// Tests that no dependency activates the "testing" feature (dev-dependencies may).
#[test]
fn test_no_testing_feature_in_business_logic() {
    let mut testing_feature_deps: Vec<_> = MEMBER_TOMLS
        .iter()
        // Ignore test-specific crates.
        .filter_map(|(package_name, toml)| {
            if CRATES_ALLOWED_TO_USE_TESTING_FEATURE.contains(&package_name.as_str()) {
                None
            } else {
                Some((package_name, toml))
            }
        })
        // Ignore crates without dependencies.
        .filter_map(|(package_name, toml)| {
            toml.dependencies.as_ref().map(|dependencies| (package_name, dependencies))
        })
        .filter_map(|(package_name, dependencies)| {
            let testing_feature_deps = dependencies
                .iter()
                .filter_map(|(name, value)| {
                    if let DependencyValue::Object { features: Some(features), .. } = value {
                        if features.contains(&"testing".to_string()) {
                            Some(name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<String>>();
            if testing_feature_deps.is_empty() {
                None
            } else {
                Some((package_name.clone(), testing_feature_deps))
            }
        })
        .collect();
    testing_feature_deps.sort();
    assert!(
        testing_feature_deps.is_empty(),
        "The following crates have (non-testing) dependencies with the 'testing' feature \
         activated. If the crate is a test crate, add it to {}.\n{testing_feature_deps:#?}",
        stringify!(CRATES_ALLOWED_TO_USE_TESTING_FEATURE)
    );
}
