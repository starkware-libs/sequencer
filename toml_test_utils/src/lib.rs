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
<<<<<<< HEAD:workspace_tests/toml_utils.rs
pub(crate) struct CrateCargoToml {
    pub(crate) package: HashMap<String, PackageEntryValue>,
    pub(crate) dependencies: Option<HashMap<String, DependencyValue>>,
||||||| 7682928ac:workspace_tests/toml_utils.rs
pub(crate) struct CrateCargoToml {
    pub(crate) package: HashMap<String, PackageEntryValue>,
    dependencies: Option<HashMap<String, DependencyValue>>,
=======
pub struct CrateCargoToml {
    pub package: HashMap<String, PackageEntryValue>,
    dependencies: Option<HashMap<String, DependencyValue>>,
>>>>>>> origin/main-v0.13.5:toml_test_utils/src/lib.rs
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<HashMap<String, DependencyValue>>,
    pub lints: Option<HashMap<String, LintValue>>,
}

impl CrateCargoToml {
<<<<<<< HEAD:workspace_tests/toml_utils.rs
    pub(crate) fn from_name(name: &String) -> Self {
        MEMBER_TOMLS.get(name).unwrap_or_else(|| panic!("No member crate '{name}' found.")).clone()
    }

    pub(crate) fn package_name(&self) -> &String {
||||||| 7682928ac:workspace_tests/toml_utils.rs
    pub(crate) fn package_name(&self) -> &String {
=======
    pub fn package_name(&self) -> &String {
>>>>>>> origin/main-v0.13.5:toml_test_utils/src/lib.rs
        match self.package.get("name") {
            Some(PackageEntryValue::String(name)) => name,
            _ => panic!("No name found in crate toml {self:?}."),
        }
    }

    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = String> + '_ {
        self.dependencies.iter().flatten().chain(self.dev_dependencies.iter().flatten()).filter_map(
            |(_name, value)| {
                if let DependencyValue::Object { path: Some(path), .. } = value {
                    Some(path.to_string())
                } else {
                    None
                }
            },
        )
    }

    /// Returns all direct member dependencies of self.
    pub(crate) fn member_dependency_names(
        &self,
        include_dev_dependencies: bool,
    ) -> HashSet<String> {
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
    pub(crate) fn member_dependency_names_recursive(
        &self,
        include_dev_dependencies: bool,
    ) -> HashSet<String> {
        self.member_dependency_names_recursive_aux(include_dev_dependencies, &mut HashSet::new())
    }
}

#[derive(Debug)]
<<<<<<< HEAD:workspace_tests/toml_utils.rs
pub(crate) struct LocalCrate {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) version: Option<String>,
||||||| 7682928ac:workspace_tests/toml_utils.rs
pub(crate) struct LocalCrate {
    pub(crate) path: String,
    pub(crate) version: String,
=======
pub struct LocalCrate {
    pub path: String,
    pub version: String,
>>>>>>> origin/main-v0.13.5:toml_test_utils/src/lib.rs
}

pub static ROOT_TOML: LazyLock<CargoToml> = LazyLock::new(|| {
    let root_toml: CargoToml =
        toml::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.toml")))
            .unwrap();
    root_toml
});
pub(crate) static MEMBER_TOMLS: LazyLock<HashMap<String, CrateCargoToml>> =
    LazyLock::new(|| ROOT_TOML.member_cargo_tomls());

impl CargoToml {
    pub fn members(&self) -> &Vec<String> {
        &self.workspace.members
    }

    pub fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }

    pub fn dependencies(&self) -> impl Iterator<Item = (&String, &DependencyValue)> + '_ {
        self.workspace.dependencies.iter()
    }

<<<<<<< HEAD:workspace_tests/toml_utils.rs
    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.dependencies().filter_map(|(name, value)| {
            if let DependencyValue::Object { path: Some(path), version, .. } = value {
                Some(LocalCrate {
                    name: name.clone(),
                    path: path.to_string(),
                    version: version.clone(),
                })
||||||| 7682928ac:workspace_tests/toml_utils.rs
    pub(crate) fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.dependencies().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), version: Some(version), .. } = value
            {
                Some(LocalCrate { path: path.to_string(), version: version.to_string() })
=======
    pub fn path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.dependencies().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), version: Some(version), .. } = value
            {
                Some(LocalCrate { path: path.to_string(), version: version.to_string() })
>>>>>>> origin/main-v0.13.5:toml_test_utils/src/lib.rs
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
                    .unwrap_or_else(|_| panic!("Failed to read {:?}", cargo_toml_path));

                let cargo_toml: CrateCargoToml = toml::from_str(&cargo_toml_content).unwrap();
                (cargo_toml.package_name().clone(), cargo_toml)
            })
            .collect()
    }
}
<<<<<<< HEAD:workspace_tests/toml_utils.rs
||||||| 7682928ac:workspace_tests/toml_utils.rs

impl CrateCargoToml {
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
=======

impl CrateCargoToml {
    pub fn path_dependencies(&self) -> impl Iterator<Item = String> + '_ {
        self.dependencies.iter().flatten().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), .. } = value {
                Some(path.to_string())
            } else {
                None
            }
        })
    }
}
>>>>>>> origin/main-v0.13.5:toml_test_utils/src/lib.rs
