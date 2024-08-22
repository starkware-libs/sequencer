use std::collections::HashMap;

use once_cell::sync::Lazy;
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

struct LocalCrate {
    name: String,
    path: String,
    version: String,
}

static ROOT_TOML: Lazy<CargoToml> = Lazy::new(|| {
    let root_toml: CargoToml =
        toml::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")))
            .unwrap();
    root_toml
});

impl CargoToml {
    fn local_crates(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.workspace.dependencies.iter().filter_map(|(name, value)| {
            if let DependencyValue::Object { path: Some(path), version } = value {
                Some(LocalCrate {
                    name: name.to_string(),
                    path: path.to_string(),
                    version: version.to_string(),
                })
            } else {
                None
            }
        })
    }

    fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }

    fn validate_no_path_dependencies(&self) {
        for LocalCrate { name, path, .. } in self.local_crates() {
            panic!(
                "Crate '{}' has a path dependency located at '{}', which is not allowed",
                name, path
            );
        }
    }
}

// Tests.

#[test]
fn test_local_dependencies_are_members() {
    for LocalCrate { path, name, .. } in ROOT_TOML.local_crates() {
        assert!(
            ROOT_TOML.workspace.members.contains(&path),
            "Crate '{name}' at path '{path}' is not a member of the workspace."
        );
    }
}

#[test]
fn test_version_alignment() {
    let workspace_version = ROOT_TOML.workspace_version();
    for LocalCrate { name, version, .. } in ROOT_TOML.local_crates() {
        assert_eq!(
            workspace_version, version,
            "Crate '{name}' has version '{version}', instead of '{workspace_version}'."
        );
    }
}

#[test]
fn test_no_path_dependencies() {
    ROOT_TOML.validate_no_path_dependencies();
}
