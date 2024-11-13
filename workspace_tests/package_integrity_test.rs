use std::path::PathBuf;

use crate::toml_utils::{PackageEntryValue, ROOT_TOML};

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
