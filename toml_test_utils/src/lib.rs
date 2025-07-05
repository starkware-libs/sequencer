use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum LintValue {
    Bool(bool),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyValue {
    String(String),
    Object { version: Option<String>, path: Option<String>, features: Option<Vec<String>> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkspaceFields {
    package: Package,
    members: Vec<String>,
    dependencies: HashMap<String, DependencyValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CargoToml {
    workspace: WorkspaceFields,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PackageEntryValue {
    String(String),
    Object { workspace: bool },
    Other(toml::Value),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrateCargoToml {
    pub package: HashMap<String, PackageEntryValue>,
    pub dependencies: Option<HashMap<String, DependencyValue>>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<HashMap<String, DependencyValue>>,
    pub lints: Option<HashMap<String, LintValue>>,
}

impl CrateCargoToml {
    pub fn from_name(name: &String) -> Self {
        MEMBER_TOMLS.get(name).unwrap_or_else(|| panic!("No member crate '{name}' found.")).clone()
    }

    pub fn package_name(&self) -> &String {
        match self.package.get("name") {
            Some(PackageEntryValue::String(name)) => name,
            _ => panic!("No name found in crate toml {self:?}."),
        }
    }

    pub fn path_dependencies(&self) -> impl Iterator<Item = String> + '_ {
        self.dependencies.iter().flatten().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), .. } = value {
                Some(path.to_string())
            } else {
                None
            }
        })
    }

    /// Returns all direct member dependencies of self.
    pub fn member_dependency_names(&self, include_dev_dependencies: bool) -> HashSet<String> {
        let member_crate_names: HashSet<&String> =
            MEMBER_TOMLS.values().map(CrateCargoToml::package_name).collect();

        self.dependencies
            .iter()
            .flatten()
            .chain(if include_dev_dependencies {
                self.dev_dependencies.iter().flatten()
            } else {
                None.iter().flatten()
            })
            .filter_map(
                |(name, _value)| {
                    if member_crate_names.contains(name) { Some(name.clone()) } else { None }
                },
            )
            .collect()
    }

    /// Helper function for member_dependency_names_recursive.
    fn member_dependency_names_recursive_aux(
        &self,
        include_dev_dependencies: bool,
        processed_member_names: &mut HashSet<String>,
    ) -> HashSet<String> {
        let direct_member_dependencies = self.member_dependency_names(include_dev_dependencies);
        let mut members = HashSet::new();
        for toml in direct_member_dependencies.iter().map(CrateCargoToml::from_name) {
            // To prevent infinite recursion, we only recurse on members that have not been
            // processed yet. If a member depends on itself, this can lead to a loop.
            let dep_name = toml.package_name();
            members.insert(dep_name.clone());
            if !processed_member_names.contains(dep_name) {
                processed_member_names.insert(dep_name.clone());
                members.extend(toml.member_dependency_names_recursive_aux(
                    include_dev_dependencies,
                    processed_member_names,
                ));
            }
        }
        members
    }

    /// Returns all member dependencies of self in the dependency tree.
    pub fn member_dependency_names_recursive(
        &self,
        include_dev_dependencies: bool,
    ) -> HashSet<String> {
        self.member_dependency_names_recursive_aux(include_dev_dependencies, &mut HashSet::new())
    }
}

#[derive(Debug)]
pub struct LocalCrate {
    pub name: String,
    pub path: String,
    pub version: Option<String>,
}

pub static ROOT_TOML: LazyLock<CargoToml> = LazyLock::new(|| {
    let root_toml: CargoToml =
        toml::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")))
            .unwrap();
    root_toml
});
pub static MEMBER_TOMLS: LazyLock<HashMap<String, CrateCargoToml>> =
    LazyLock::new(|| ROOT_TOML.member_cargo_tomls());

impl CargoToml {
    pub fn members(&self) -> &Vec<String> {
        &self.workspace.members
    }

    pub fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }

    pub fn contains_dependency(&self, name: &str) -> bool {
        self.workspace.dependencies.contains_key(name)
    }

    pub fn dependencies(&self) -> impl Iterator<Item = (&String, &DependencyValue)> + '_ {
        self.workspace.dependencies.iter()
    }

    pub fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.dependencies().filter_map(|(name, value)| {
            if let DependencyValue::Object { path: Some(path), version, .. } = value {
                Some(LocalCrate {
                    name: name.clone(),
                    path: path.to_string(),
                    version: version.clone(),
                })
            } else {
                None
            }
        })
    }

    pub fn member_cargo_tomls(&self) -> HashMap<String, CrateCargoToml> {
        let crates_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../"));
        self.members()
            .iter()
            .map(|member| {
                let cargo_toml_path = crates_dir.join(member).join("Cargo.toml");

                let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
                    .unwrap_or_else(|_| panic!("Failed to read {cargo_toml_path:?}"));

                let cargo_toml: CrateCargoToml = toml::from_str(&cargo_toml_content).unwrap();
                (cargo_toml.package_name().clone(), cargo_toml)
            })
            .collect()
    }
}
