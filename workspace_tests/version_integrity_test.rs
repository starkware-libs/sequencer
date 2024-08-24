use crate::toml_utils::{LocalCrate, ROOT_TOML};

#[test]
fn test_path_dependencies_are_members() {
    let non_member_path_crates: Vec<_> = ROOT_TOML
        .path_dependencies()
        .filter(|LocalCrate { path, .. }| !ROOT_TOML.members().contains(path))
        .collect();
    assert!(
        non_member_path_crates.is_empty(),
        "The following crates are path dependencies but not members of the workspace: \
         {non_member_path_crates:?}."
    );
}

#[test]
fn test_version_alignment() {
    let workspace_version = ROOT_TOML.workspace_version();
    let crates_with_incorrect_version: Vec<_> = ROOT_TOML
        .path_dependencies()
        .filter(|LocalCrate { version, .. }| version != workspace_version)
        .collect();
    assert!(
        crates_with_incorrect_version.is_empty(),
        "The following crates have versions different from the workspace version \
         '{workspace_version}': {crates_with_incorrect_version:?}."
    );
}

#[test]
fn validate_all_cargo_tomls() {
    for member in &ROOT_TOML.workspace.members {
        println!("Validating Cargo.toml for member: {}", member);
        let cargo_toml = read_cargo_toml(member);
        let crate_paths: Vec<LocalCrate> =
            cargo_toml.validate_no_crate_path_dependencies().collect();
        assert!(
            crate_paths.is_empty(),
            "The following crates have path dependency {crate_paths:?}."
        );
    }
}
