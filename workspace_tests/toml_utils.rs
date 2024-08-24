use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DependencyValue {
    String(String),
    Object { version: String, path: Option<String> },
    CrateObject { workspace: Option<bool>, features: Option<Vec<String>>, path: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Package {
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkspaceFields {
    package: Package,
    members: Vec<String>,
    dependencies: HashMap<String, DependencyValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CargoToml {
    workspace: Option<WorkspaceFields>,
    dependencies: Option<HashMap<String, DependencyValue>>,
}

#[derive(Debug)]
pub(crate) struct LocalCrate {
    pub(crate) path: String,
    pub(crate) version: String,
}

pub(crate) static ROOT_TOML: LazyLock<CargoToml> = LazyLock::new(|| {
    let root_toml: CargoToml =
        toml::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")))
            .unwrap();
    root_toml
});

impl CargoToml {
    pub(crate) fn has_dependencies(&self) -> bool {
        self.dependencies.is_some()
    }

    pub(crate) fn members(&self) -> &Vec<String> {
        if let Some(workspace) = &self.workspace {
            &workspace.members
        } else {
            panic!("No workspace exist");
        }
    }

    pub(crate) fn workspace_version(&self) -> &str {
        if let Some(workspace) = &self.workspace {
            &workspace.package.version
        } else {
            panic!("No workspace exist");
        }
    }

    pub(crate) fn workspace_path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        if let Some(workspace) = &self.workspace {
            workspace.dependencies.iter().filter_map(|(_name, value)| {
                if let DependencyValue::Object { path: Some(path), version } = value {
                    Some(LocalCrate { path: path.to_string(), version: version.to_string() })
                } else {
                    None
                }
            })
        } else {
            panic!("No workspace exist");
        }
    }

    pub(crate) fn crate_path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        if let Some(dependencies) = &self.dependencies {
            dependencies.iter().filter_map(|(_name, value)| {
                if let DependencyValue::Object { path: Some(path), version } = value {
                    Some(LocalCrate { path: path.to_string(), version: version.to_string() })
                } else {
                    None
                }
            })
        } else {
            panic!("No dependencies exist");
        }
    }
}

pub(crate) fn read_cargo_toml(member: &str) -> CargoToml {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let crates_dir = format!("{}/../", manifest_dir);

    let cargo_toml_path = Path::new(&crates_dir).join(member).join("Cargo.toml");

    let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
        .expect(&format!("Failed to read {:?}", cargo_toml_path));

    toml::from_str(&cargo_toml_content).unwrap()
}
