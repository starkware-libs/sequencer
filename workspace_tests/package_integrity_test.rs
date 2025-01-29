use std::path::PathBuf;

use crate::toml_utils::{CrateCargoToml, DependencyValue, PackageEntryValue, ROOT_TOML};

#[test]
fn test_package_names_match_directory() {
    let mismatched_packages: Vec<_> = ROOT_TOML
        .member_cargo_tomls()
        .into_iter()
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
    let member_tomls = ROOT_TOML.member_cargo_tomls();
    let member_crate_names: Vec<&String> =
        member_tomls.values().map(CrateCargoToml::package_name).collect();
    let package_to_bad_local_dev_deps: Vec<(String, Vec<String>)> = ROOT_TOML
        .member_cargo_tomls()
        .into_iter()
        .filter_map(|(path_str, toml)| {
            if let Some(dev_dependencies) = toml.dev_dependencies {
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
                    Some((path_str, bad_local_dev_deps))
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
