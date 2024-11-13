use std::path::PathBuf;

use crate::toml_utils::ROOT_TOML;

#[test]
fn test_package_names_match_directory() {
    let mismatched_packages: Vec<_> = ROOT_TOML
        .member_cargo_tomls()
        .into_iter()
        .filter_map(|(path_str, toml)| {
            let path = PathBuf::from(&path_str);
            let directory_name = path.file_name()?.to_str()?;
            if toml.package.name != directory_name {
                Some((path_str, toml.package.name.clone()))
            } else {
                None
            }
        })
        .collect();
    assert!(
        mismatched_packages.is_empty(),
        "The following crates have package names that do not match their directory names: \
         {mismatched_packages:?}."
    );
}
