use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DependencyValue {
    String(String),
    Object { version: String, path: Option<String> },
    Bool(bool),
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
    pub(crate) fn workspace_path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        self.workspace.dependencies.iter().filter_map(|(_name, value)| {
            if let DependencyValue::Object { path: Some(path), version } = value {
                Some(LocalCrate { path: path.to_string(), version: version.to_string() })
            } else {
                None
            }
        })
    }

    pub(crate) fn members(&self) -> &Vec<String> {
        &self.workspace.members
    }

    pub(crate) fn workspace_version(&self) -> &str {
        &self.workspace.package.version
    }

    fn validate_no_crate_path_dependencies(&self) -> impl Iterator<Item = LocalCrate> + '_ {
        println!("Validating no crate path dependencies for {:?}", self.dependencies);

        // Ensure dependencies exist
        if let Some(dependencies) = &self.dependencies {
            // If dependencies exist, iterate over them
            dependencies.iter().filter_map(|(_name, value)| {
                if let DependencyValue::Object { path: Some(path), version } = value {
                    Some(LocalCrate { path: path.to_string(), version: version.to_string() })
                } else {
                    None
                }
            })
        } else {
            panic!("No dependencies found in Cargo.toml");
        }
        // Check if any dependency has a path attribute
        // !self
        //     .dependencies
        //     .iter()
        //     .any(|(_name, value)| matches!(value, DependencyValue::Object { path: Some(_), .. }))
    }
}

fn print_directory_contents(dir: &str) -> io::Result<()> {
    // Convert the directory path to a Path
    let path = Path::new(dir);

    // Read the directory contents
    let entries = fs::read_dir(path)?;

    // Iterate over each entry in the directory
    println!("************* Directory {} contents:", dir);
    for entry in entries {
        // println!("Entry: {:?}", entry);
        let entry = entry?;
        let file_name = entry.file_name();
        println!("{}", file_name.to_string_lossy());
    }

    Ok(())
}

fn read_cargo_toml(member: &str) -> CargoToml {
    // Get the path to the current workspace directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    print_directory_contents(manifest_dir).expect("Failed to read directory contents");
    let crates_dir = format!("{}/../", manifest_dir);
    let mempool_crates_dir = format!("{}/../crates/mempool", manifest_dir);
    print_directory_contents(&crates_dir).expect("Failed to read directory contents");
    print_directory_contents(&mempool_crates_dir).expect("Failed to read directory contents");
    println!("ROOT_TOML: {:?}", ROOT_TOML);
    println!("manifest_dir: {}", manifest_dir);
    println!("Reading Cargo.toml for member: {}", member);

    // Dynamically construct the path to the member's Cargo.toml file
    let cargo_toml_path = Path::new(&crates_dir).join(member).join("Cargo.toml");
    println!("*****************");
    println!("Reading cargo_toml_path {:?}", cargo_toml_path);

    // Read the contents of the Cargo.toml file
    let cargo_toml_content = fs::read_to_string(&cargo_toml_path)
        .expect(&format!("Failed to read {:?}", cargo_toml_path));

    // Parse the content into the CargoToml struct using toml::from_str
    toml::from_str(&cargo_toml_content).expect(&format!("Failed to parse {:?}", cargo_toml_path))
}
