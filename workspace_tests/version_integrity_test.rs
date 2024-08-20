use std::collections::HashMap;

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

impl CargoToml {
    /// Deserialize the root cargo toml.
    fn load() -> Self {
        let root_toml: CargoToml = toml::from_str(include_str!("../Cargo.toml")).unwrap();
        root_toml
    }

    fn local_crates(&self) -> Vec<LocalCrate> {
        self.workspace
            .dependencies
            .iter()
            .filter_map(|(name, value)| {
                if let DependencyValue::Object { path, version } = value {
                    if let Some(path) = path {
                        return Some(LocalCrate {
                            name: name.to_string(),
                            path: path.to_string(),
                            version: version.to_string(),
                        });
                    }
                }
                None
            })
            .collect()
    }

    fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }
}

// Tests.

#[test]
fn test_local_dependencies_are_members() {
    let root_toml = CargoToml::load();
    for LocalCrate { path, name, .. } in root_toml.local_crates().iter() {
        assert!(
            root_toml.workspace.members.contains(&path),
            "Crate '{name}' at path '{path}' is not a member of the workspace."
        );
    }
}

#[test]
fn test_version_alignment() {
    let root_toml = CargoToml::load();
    let workspace_version = root_toml.workspace_version();
    for LocalCrate { name, version, .. } in root_toml.local_crates().iter() {
        assert_eq!(
            workspace_version, version,
            "Crate '{name}' has version '{version}', instead of '{workspace_version}'."
        );
    }
}
