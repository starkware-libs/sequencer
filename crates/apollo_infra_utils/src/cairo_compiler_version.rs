use cached::proc_macro::cached;
use serde::{Deserialize, Serialize};

/// Objects for simple deserialization of `Cargo.toml`, to fetch the Cairo1 compiler version.
/// The compiler itself isn't actually a dependency, so we compile by using the version of the
/// `cairo-lang-casm` crate.
/// The choice of this crate is arbitrary, as all compiler crate dependencies should have the
/// same version.
/// Deserializes:
/// """
/// ...
/// [workspace.dependencies]
/// ...
/// cairo-lang-casm = VERSION
/// ...
/// """
/// where `VERSION` can be a simple "x.y.z" version string or an object with a "version" field.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum DependencyValue {
    // cairo-lang-casm = "x.y.z".
    String(String),
    // cairo-lang-casm = { version = "x.y.z", .. }.
    Object { version: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct CairoLangCasmDependency {
    #[serde(rename = "cairo-lang-casm")]
    cairo_lang_casm: DependencyValue,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceFields {
    dependencies: CairoLangCasmDependency,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoToml {
    workspace: WorkspaceFields,
}

#[cached]
/// Returns the version of the Cairo1 compiler defined in the root Cargo.toml (by checking the
/// package version of one of the crates from the compiler in the dependencies).
pub fn cairo1_compiler_version() -> String {
    let cargo_toml: CargoToml = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    match cargo_toml.workspace.dependencies.cairo_lang_casm {
        DependencyValue::String(version) | DependencyValue::Object { version } => {
            version.clone().trim_start_matches("=").to_string()
        }
    }
}
