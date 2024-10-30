use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use cached::proc_macro::cached;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

const CAIRO0_PIP_REQUIREMENTS_FILE: &str = "tests/requirements.txt";
const CAIRO1_REPO_RELATIVE_PATH_OVERRIDE_ENV_VAR: &str = "CAIRO1_REPO_RELATIVE_PATH";
const DEFAULT_CAIRO1_REPO_RELATIVE_PATH: &str = "../../../cairo";

/// Objects for simple deserialization of Cargo.toml to fetch the Cairo1 compiler version.
/// The compiler itself isn't actually a dependency, so we compile by using the version of the
/// cairo-lang-casm crate.
/// The choice of cairo-lang-casm is arbitrary, as all compiler crate dependencies should have the
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
    let cargo_toml: CargoToml = toml::from_str(include_str!("../../../../Cargo.toml")).unwrap();
    match cargo_toml.workspace.dependencies.cairo_lang_casm {
        DependencyValue::String(version) | DependencyValue::Object { version } => version.clone(),
    }
}

pub fn cairo1_compiler_tag() -> String {
    format!("v{}", cairo1_compiler_version())
}

/// Returns the path to the local Cairo1 compiler repository.
/// Returns <sequencer_repo_root>/<RELATIVE_PATH_TO_CAIRO_REPO>, where the relative path can be
/// overridden by the environment variable (otherwise, the default is used).
fn local_cairo1_compiler_repo_path() -> PathBuf {
    // Location of blockifier's Cargo.toml.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    Path::new(&manifest_dir).join(
        env::var(CAIRO1_REPO_RELATIVE_PATH_OVERRIDE_ENV_VAR)
            .unwrap_or_else(|_| DEFAULT_CAIRO1_REPO_RELATIVE_PATH.into()),
    )
}

/// Runs a command. If it has succeeded, it returns the command's output; otherwise, it panics with
/// stderr output.
fn run_and_verify_output(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    if !output.status.success() {
        let stderr_output = String::from_utf8(output.stderr).unwrap();
        panic!("{stderr_output}");
    }
    output
}

/// Compiles a Cairo0 program using the deprecated compiler.
pub fn cairo0_compile(path: String, extra_arg: Option<String>, debug_info: bool) -> Vec<u8> {
    verify_cairo0_compiler_deps();
    let mut command = Command::new("starknet-compile-deprecated");
    command.arg(&path);
    if let Some(extra_arg) = extra_arg {
        command.arg(extra_arg);
    }
    if !debug_info {
        command.arg("--no_debug_info");
    }
    let compile_output = command.output().unwrap();
    let stderr_output = String::from_utf8(compile_output.stderr).unwrap();
    assert!(compile_output.status.success(), "{stderr_output}");
    compile_output.stdout
}

/// Compiles a Cairo1 program using the compiler version set in the Cargo.toml.
pub fn cairo1_compile(
    path: String,
    git_tag_override: Option<String>,
    cargo_nightly_arg: Option<String>,
) -> Vec<u8> {
    let cairo1_compiler_path = local_cairo1_compiler_repo_path();

    // Command args common to both compilation phases.
    let mut base_compile_args = vec![
        "run".into(),
        format!("--manifest-path={}/Cargo.toml", cairo1_compiler_path.to_string_lossy()),
        "--bin".into(),
    ];
    // Add additional cargo arg if provided. Should be first arg (base command is `cargo`).
    if let Some(ref nightly_version) = cargo_nightly_arg {
        base_compile_args.insert(0, format!("+nightly-{nightly_version}"));
    }

    let sierra_output = starknet_compile(path, git_tag_override, cargo_nightly_arg);

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(&sierra_output).unwrap();
    let temp_path_str = temp_file.into_temp_path();

    // Sierra -> CASM.
    let mut sierra_compile_command = Command::new("cargo");
    sierra_compile_command.args(base_compile_args);
    sierra_compile_command.args(["starknet-sierra-compile", temp_path_str.to_str().unwrap()]);
    let casm_output = run_and_verify_output(&mut sierra_compile_command);

    casm_output.stdout
}

pub fn starknet_compile(
    path: String,
    git_tag_override: Option<String>,
    cargo_nightly_arg: Option<String>,
) -> Vec<u8> {
    prepare_cairo1_compiler_deps(git_tag_override);

    let cairo1_compiler_path = local_cairo1_compiler_repo_path();

    // Command args common to both compilation phases.
    let mut base_compile_args = vec![
        "run".into(),
        format!("--manifest-path={}/Cargo.toml", cairo1_compiler_path.to_string_lossy()),
        "--bin".into(),
    ];
    // Add additional cargo arg if provided. Should be first arg (base command is `cargo`).
    if let Some(nightly_version) = cargo_nightly_arg {
        base_compile_args.insert(0, format!("+nightly-{nightly_version}"));
    }

    // Cairo -> Sierra.
    let mut starknet_compile_commmand = Command::new("cargo");
    starknet_compile_commmand.args(base_compile_args.clone());
    starknet_compile_commmand.args(["starknet-compile", "--", "--single-file", &path]);
    let sierra_output = run_and_verify_output(&mut starknet_compile_commmand);

    sierra_output.stdout
}

fn prepare_cairo1_compiler_deps(git_tag_override: Option<String>) {
    let cairo_repo_path = local_cairo1_compiler_repo_path();
    let tag = git_tag_override.unwrap_or(cairo1_compiler_tag());

    // Check if the path is a directory.
    assert!(
        cairo_repo_path.is_dir(),
        "Cannot verify Cairo1 contracts, Cairo repo not found at {0}.\nPlease run:\n\
        git clone https://github.com/starkware-libs/cairo {0}\nThen rerun the test.",
        cairo_repo_path.to_string_lossy(),
    );

    // Checkout the required version in the compiler repo.
    run_and_verify_output(Command::new("git").args([
        "-C",
        cairo_repo_path.to_str().unwrap(),
        "checkout",
        &tag,
    ]));

    // Verify that the checked out tag is as expected.
    run_and_verify_output(Command::new("git").args([
        "-C",
        cairo_repo_path.to_str().unwrap(),
        "rev-parse",
        "--verify",
        &tag,
    ]));
}
