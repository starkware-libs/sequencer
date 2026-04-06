use serde::{Deserialize, Serialize};

use crate::cairo_compiler_version::CAIRO1_COMPILER_VERSION;

/// Cross-checks that `CAIRO1_COMPILER_VERSION` (loaded from `cairo_compiler_version.txt`,
/// which the install scripts also read) matches the `cairo-lang-casm` version pinned in
/// `Cargo.toml`. The two are managed independently: the `.txt` file drives the external
/// compiler binary install, the Cargo dep drives the in-process Sierra crates. They must
/// stay in sync; this test fires on drift.
/// The choice of `cairo-lang-casm` is arbitrary, as all compiler crate dependencies should
/// have the same version.
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

#[test]
fn test_current_compiler_version() {
    let cargo_toml: CargoToml = toml::from_str(include_str!("../../../Cargo.toml")).unwrap();
    let actual_version = match cargo_toml.workspace.dependencies.cairo_lang_casm {
        DependencyValue::String(version) | DependencyValue::Object { version } => {
            version.trim_start_matches("=").to_string()
        }
    };
    assert_eq!(CAIRO1_COMPILER_VERSION, actual_version);
}
