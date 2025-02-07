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
#[serde(untagged)]
pub(crate) enum PackageEntryValue {
    String(String),
    Object { workspace: bool },
    Other(toml::Value),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CrateCargoToml {
    pub(crate) package: HashMap<String, PackageEntryValue>,
    pub(crate) dependencies: Option<HashMap<String, DependencyValue>>,
    #[serde(rename = "dev-dependencies")]
    pub(crate) dev_dependencies: Option<HashMap<String, DependencyValue>>,
    pub(crate) lints: Option<HashMap<String, LintValue>>,
}

impl CrateCargoToml {
    pub(crate) fn package_name(&self) -> &String {
        match self.package.get("name") {
            Some(PackageEntryValue::String(name)) => name,
            _ => panic!("No name found in crate toml {self:?}."),
        }
    }

    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = String> + '_ {
        self.dependencies.iter().flatten().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), .. } = value {
                Some(path.to_string())
            } else {
                None
            }
        })
    }
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
pub(crate) static MEMBER_TOMLS: LazyLock<HashMap<String, CrateCargoToml>> =
    LazyLock::new(|| ROOT_TOML.member_cargo_tomls());

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
                (cargo_toml.package_name().clone(), cargo_toml)
            })
            .collect()
    }
}
