use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum LintValue {
    Bool(bool),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DependencyValue {
    String(String),
    Object { version: Option<String>, path: Option<String>, features: Option<Vec<String>> },
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
    workspace: WorkspaceFields,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum CrateVersion {
    Version(String),
    #[serde(rename = "workspace")]
    Workspace(bool),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CrateCargoTomlPackage {
    pub(crate) version: CrateVersion,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CrateCargoToml {
    pub(crate) package: CrateCargoTomlPackage,
    dependencies: Option<HashMap<String, DependencyValue>>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: Option<HashMap<String, DependencyValue>>,
    pub(crate) lints: Option<HashMap<String, LintValue>>,
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
    pub(crate) fn members(&self) -> &Vec<String> {
        &self.workspace.members
    }

    pub(crate) fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }

    pub(crate) fn dependencies(&self) -> impl Iterator<Item = (&String, &DependencyValue)> + '_ {
        self.workspace.dependencies.iter()
    }

    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.dependencies().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), version: Some(version), .. } = value
            {
                Some(LocalCrate { path: path.to_string(), version: version.to_string() })
            } else {
                None
            }
        })
    }

    pub(crate) fn member_cargo_tomls(&self) -> HashMap<String, CrateCargoToml> {
        let crates_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../"));
        self.members()
            .iter()
            .map(|member| {
                let cargo_toml_path = crates_dir.join(member).join("Cargo.toml");

                let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
                    .unwrap_or_else(|_| panic!("Failed to read {:?}", cargo_toml_path));

                let cargo_toml: CrateCargoToml = toml::from_str(&cargo_toml_content).unwrap();
                (member.clone(), cargo_toml)
            })
            .collect()
    }
}

impl CrateCargoToml {
    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = String> + '_ {
        self.dependencies.iter().chain(self.dev_dependencies.iter()).flatten().filter_map(
            |(_name, value)| {
                if let DependencyValue::Object { path: Some(path), .. } = value {
                    Some(path.to_string())
                } else {
                    None
                }
            },
        )
    }
}
