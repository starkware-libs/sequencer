use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use toml_test_utils::{
    CrateCargoToml,
    DependencyValue,
    LocalCrate,
    PackageEntryValue,
    MEMBER_TOMLS,
    ROOT_TOML,
};

const PARENT_BRANCH: &str = include_str!("../scripts/parent_branch.txt");
const MAIN_PARENT_BRANCH: &str = "main";
const EXPECTED_MAIN_VERSION: &str = "0.0.0";

static ROOT_CRATES_FOR_PUBLISH: LazyLock<HashSet<&str>> =
    LazyLock::new(|| HashSet::from(["blockifier", "apollo_starknet_os_program"]));
static CRATES_FOR_PUBLISH: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let publish_deps: HashSet<String> = ROOT_CRATES_FOR_PUBLISH
        .iter()
        .flat_map(|crate_name| {
            // No requirement to publish dev dependencies.
            CrateCargoToml::from_name(&crate_name.to_string())
                .member_dependency_names_recursive(false)
        })
        .collect();
    publish_deps
        .union(&ROOT_CRATES_FOR_PUBLISH.iter().map(|s| s.to_string()).collect())
        .cloned()
        .collect()
});

/// All member crates listed in the root Cargo.toml should have a version field if and only if they
/// are intended for publishing.
/// To understand why the workspace benefits from this check, consider the following scenario:
/// Say crates X, Y and Z are members of the workspace, and crates/X/Cargo.toml is:
/// ```toml
/// [package]
/// name = "X"
///
/// [dependencies]
/// Y.workspace = true
///
/// [dev-dependencies]
/// Z.workspace = true
/// ```
/// Consider the (problematic) contents of the root Cargo.toml:
/// ```toml
/// X = { path = "crates/X", version = "1.2.3" }
/// Y = { path = "crates/Y", version = "1.2.3" }
/// Z = { path = "crates/Z", version = "1.2.3" }
/// ```
/// If X is intended for publishing, both X and Y must have a valid version field. Z is not required
/// for publishing, because it is only a dev dependency. However, since it has a version field,
/// `cargo publish -p X` will fail because Z is not published.
/// If the root Cargo.toml is:
/// ```toml
/// X = { path = "crates/X", version = "1.2.3" }
/// Y = { path = "crates/Y", version = "1.2.3" }
/// Z.path = "crates/Z"
/// ```
/// then `cargo publish -p X` will succeed, because the command ignores path dependencies without
/// version fallbacks.
#[test]
fn test_members_have_version_iff_they_are_for_publish() {
    let members_with_version: HashSet<String> = ROOT_TOML
        .path_dependencies()
        .filter_map(
            |LocalCrate { name, version, .. }| {
                if version.is_some() { Some(name.clone()) } else { None }
            },
        )
        .collect();
    let members_without_version: HashSet<String> = ROOT_TOML
        .path_dependencies()
        .filter_map(
            |LocalCrate { name, .. }| {
                if !members_with_version.contains(&name) { Some(name) } else { None }
            },
        )
        .collect();

    let mut published_crates_without_version: Vec<String> =
        members_without_version.intersection(&*CRATES_FOR_PUBLISH).cloned().collect();
    let mut unpublished_crates_with_version: Vec<String> =
        members_with_version.difference(&*CRATES_FOR_PUBLISH).cloned().collect();
    published_crates_without_version.sort();
    unpublished_crates_with_version.sort();
    assert!(
        published_crates_without_version.is_empty() && unpublished_crates_with_version.is_empty(),
        "The following crates are missing a version field in the workspace Cargo.toml: \
         {published_crates_without_version:#?}.\nThe following crates have a version field but \
         are not intended for publishing: {unpublished_crates_with_version:#?}."
    );
}

#[test]
fn test_members_are_deps() {
    let member_tomls = ROOT_TOML.member_cargo_tomls();
    let non_dep_members: Vec<_> =
        member_tomls.keys().filter(|member| !ROOT_TOML.contains_dependency(member)).collect();
    assert!(
        non_dep_members.is_empty(),
        "The following crates are members of the workspace but not dependencies: \
         {non_dep_members:?}."
    );
}

#[test]
fn test_members_have_paths() {
    let member_tomls = ROOT_TOML.member_cargo_tomls();
    let path_dependencies_names: HashSet<String> =
        ROOT_TOML.path_dependencies().map(|dep| dep.name).collect();
    let members_without_paths: Vec<_> = member_tomls
        .keys()
        .filter(|member| !path_dependencies_names.contains(&member.to_string()))
        .collect();
    assert!(
        members_without_paths.is_empty(),
        "The following crates are members of the workspace but do not have a path: \
         {members_without_paths:?}."
    );
}

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
        .filter(
            |LocalCrate { version, .. }| {
                if let Some(version) = version { version != workspace_version } else { false }
            },
        )
        .collect();
    assert!(
        crates_with_incorrect_version.is_empty(),
        "The following crates have versions different from the workspace version \
         '{workspace_version}': {crates_with_incorrect_version:?}."
    );
}

#[test]
fn validate_crate_version_is_workspace() {
    let crates_without_workspace_version: Vec<String> = MEMBER_TOMLS
        .iter()
        .flat_map(|(member, toml)| match toml.package.get("version") {
            // No `version` field.
            None => Some(member.clone()),
            Some(version) => match version {
                // version = "x.y.z".
                PackageEntryValue::String(_) => Some(member.clone()),
                // version.workspace = (true | false).
                PackageEntryValue::Object { workspace } => {
                    if *workspace {
                        None
                    } else {
                        Some(member.clone())
                    }
                }
                // Unknown version object.
                PackageEntryValue::Other(_) => Some(member.clone()),
            },
        })
        .collect();

    assert!(
        crates_without_workspace_version.is_empty(),
        "The following crates don't have `version.workspace = true` in the [package] section: \
         {crates_without_workspace_version:?}."
    );
}

#[test]
fn validate_no_path_dependencies() {
    let all_path_deps_in_crate_tomls: HashMap<String, String> = MEMBER_TOMLS
        .iter()
        .filter_map(|(crate_name, toml)| {
            let path_deps: Vec<String> = toml.path_dependencies().collect();
            if path_deps.is_empty() {
                None
            } else {
                Some((crate_name.clone(), path_deps.join(", ")))
            }
        })
        .collect();

    assert!(
        all_path_deps_in_crate_tomls.is_empty(),
        "The following crates have path dependency {all_path_deps_in_crate_tomls:?}."
    );
}

#[test]
fn test_no_features_in_workspace() {
    let dependencies_with_features: Vec<_> = ROOT_TOML
        .dependencies()
        .filter_map(|(name, dependency)| match dependency {
            DependencyValue::Object { features: Some(features), .. } => Some((name, features)),
            _ => None,
        })
        .collect();
    assert!(
        dependencies_with_features.is_empty(),
        "The following dependencies have features enabled in the workspace Cargo.toml: \
         {dependencies_with_features:#?}. Features should only be activated in the crate that \
         needs them."
    );
}

#[test]
fn test_main_branch_is_versionless() {
    if PARENT_BRANCH.trim() == MAIN_PARENT_BRANCH {
        let workspace_version = ROOT_TOML.workspace_version();
        assert_eq!(
            workspace_version, EXPECTED_MAIN_VERSION,
            "The workspace version should be '{EXPECTED_MAIN_VERSION}' when the parent branch is \
             '{MAIN_PARENT_BRANCH}'; found {workspace_version}.",
        );
    }
}
