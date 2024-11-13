use std::path::PathBuf;

use crate::toml_utils::{PackageEntryValue, ROOT_TOML};

#[test]
fn test_package_names_match_directory() {
    let mismatched_packages: Vec<_> =
        ROOT_TOML
            .member_cargo_tomls()
            .into_iter()
            .filter_map(|(path_str, toml)| {
                let path = PathBuf::from(&path_str);
                let directory_name = path.file_name()?.to_str()?;
                match toml.package.get("name") {
                    // No package name.
                    None => Some(path_str),
                    // Package has a valid name; check match.
                    Some(PackageEntryValue::String(package_name)) => {
                        if package_name != directory_name { Some(path_str) } else { None }
                    }
                    // Unknown name object.
                    Some(_) => Some(path_str),
                }
            })
            .collect();
    assert!(
        mismatched_packages.is_empty(),
        "The following crates have package names that do not match their directory names: \
         {mismatched_packages:?}."
    );
}
