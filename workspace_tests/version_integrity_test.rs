use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum DependencyValue {
    String(String),
    Object { version: String, path: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Package {
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorkspaceFields {
    package: Package,
    members: Vec<String>,
    dependencies: HashMap<String, DependencyValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CargoToml {
    workspace: WorkspaceFields,
}

#[derive(Debug)]
struct LocalCrate {
    path: String,
    version: String,
}

static ROOT_TOML: LazyLock<CargoToml> = LazyLock::new(|| {
    let root_toml: CargoToml =
        toml::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")))
            .unwrap();
    root_toml
});

impl CargoToml {
    fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.workspace.dependencies.iter().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), version } = value {
                Some(LocalCrate { path: path.to_string(), version: version.to_string() })
            } else {
                None
            }
        })
    }

    fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }
}

// Tests.

#[test]
fn test_path_dependencies_are_members() {
    let non_member_path_crates: Vec<_> = ROOT_TOML
        .path_dependencies()
        .filter(|LocalCrate { path, .. }| !ROOT_TOML.workspace.members.contains(&path))
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
