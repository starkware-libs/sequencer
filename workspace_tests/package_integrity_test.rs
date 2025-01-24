use std::path::PathBuf;

use crate::toml_utils::{PackageEntryValue, MEMBER_TOMLS};

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
