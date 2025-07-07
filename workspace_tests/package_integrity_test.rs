use std::path::PathBuf;

use toml_test_utils::{DependencyValue, PackageEntryValue, MEMBER_TOMLS};

/// Hard-coded list of crates that are allowed to use test code in their (non-dev) dependencies.
/// Should only contain test-related crates.
static CRATES_ALLOWED_TO_USE_TESTING_FEATURE: [&str; 6] = [
    "apollo_integration_tests",
    "apollo_test_utils",
    "blockifier_test_utils",
    "papyrus_load_test",
    "mempool_test_utils",
    // The CLI crate exposes tests that require test utils in dependencies.
    // TODO(Dori): Consider splitting the build of the CLI crate to a test build and a production
    //   build.
    "starknet_committer_and_os_cli",
];

/// Tests that no member crate has itself in it's dependency tree --- a crate cannot be published
/// with a self dependenecy (unless one publishes with `--no-verify`). Note: dev-dependencies are
/// not part of the publish published unless they a version, which most of our crates have.
/// When writing unit tests, one should feature-gate test code with `any(test,features=testing)`,
/// thus self deps should not be an issue. However, when writing cargo integration tests this gets
/// murky: `test` isn't enabled so `testing` must be used, but a current limitation of cargo allows
/// this only through self-deps:
/// https://github.com/rust-lang/cargo/issues/2911#issuecomment-1739880593.
/// But to make matters worse, a recent bug in cargo >= 1.84 makes this no longer work:
/// https://github.com/rust-lang/cargo/issues/15151
/// making it impossible to write integration tests that use the `testing` feature. Our current
/// work around is to write integration tests that require features as integration tests inside the
/// apollo_integration_tests crate.
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
