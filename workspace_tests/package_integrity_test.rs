use std::path::PathBuf;

use crate::toml_utils::{CrateCargoToml, DependencyValue, PackageEntryValue, MEMBER_TOMLS};

static CRATES_ALLOWED_TO_USE_TEST_CODE: [&str; 6] = [
    "starknet_integration_tests",
    "papyrus_test_utils",
    "blockifier_test_utils",
    "papyrus_load_test",
    "mempool_test_utils",
    // The CLI crate exposes tests that require test utils in dependencies.
    // TODO(Dori): Consider splitting the build of the CLI crate to a test build and a production
    //   build.
    "starknet_committer_and_os_cli",
];

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
            if CRATES_ALLOWED_TO_USE_TEST_CODE.contains(&package_name.as_str()) {
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
        stringify!(CRATES_ALLOWED_TO_USE_TEST_CODE)
    );
}

/// For smooth publishing: all *local* (workspace member) dependencies in [dev-dependencies] should
/// be via path, and not `workspace = true`.
/// Reason:
/// - `cargo publish` ignores path dependencies in dev-dependencies.
/// - `cargo publish` DOES NOT ignore `workspace = true` dependencies in dev-dependencies, even if
///   the workspace toml defines the dependency via path.
/// - We do not need dev-dependencies published when publishing a crate.
/// - We sometimes use self-references in dev-dependencies to activate features of self in tests.
///   For example, starknet_api needs it's "testing" feature activated in test mode, so it has a
///   dependency on self (starknet_api) in dev-dependencies. If we fail to ignore this self
///   dependency, we will not be able to publish starknet_api.
#[test]
fn test_member_dev_dependencies_are_by_path() {
    let member_crate_names: Vec<&String> =
        MEMBER_TOMLS.values().map(CrateCargoToml::package_name).collect();
    let package_to_bad_local_dev_deps: Vec<(String, Vec<String>)> = MEMBER_TOMLS
        .iter()
        .filter_map(|(path_str, toml)| {
            if let Some(ref dev_dependencies) = toml.dev_dependencies {
                // For each dep in dev-dependencies: if it's a workspace member, and not by path,
                // add it to the list of bad local dev deps.
                let mut bad_local_dev_deps: Vec<String> = dev_dependencies
                    .iter()
                    .filter_map(|(name, value)| {
                        if member_crate_names.contains(&name) {
                            match value {
                                DependencyValue::Object { path: Some(_), .. } => None,
                                _ => Some(name.clone()),
                            }
                        } else {
                            None
                        }
                    })
                    .collect();
                // If bad dev-deps exist for this package, return the package name and the bad deps.
                if bad_local_dev_deps.is_empty() {
                    None
                } else {
                    bad_local_dev_deps.sort();
                    Some((path_str.clone(), bad_local_dev_deps))
                }
            } else {
                None
            }
        })
        .collect();
    assert!(
        package_to_bad_local_dev_deps.is_empty(),
        "The following crates have local packages in the [dev-dependencies] area that are not by \
         path: {package_to_bad_local_dev_deps:#?}."
    );
}
