use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use apollo_infra_utils::cairo0_compiler::Cairo0Script;
use apollo_infra_utils::cairo0_compiler_test_utils::verify_cairo0_compiler_deps;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_infra_utils::path::{project_path, resolve_project_relative_path};
use tempfile::NamedTempFile;
use tracing::info;

pub enum CompilationArtifacts {
    Cairo0 { casm: Vec<u8> },
    Cairo1 { casm: Vec<u8>, sierra: Vec<u8> },
}

pub fn cairo1_compiler_tag() -> String {
    format!("v{CAIRO1_COMPILER_VERSION}")
}

/// Path to local compiler package directory, of the specified version.
fn cairo1_package_dir(version: &String) -> PathBuf {
    project_path().unwrap().join(format!("target/bin/cairo_package__{version}"))
}

/// Path to starknet-compile binary, of the specified version.
fn starknet_compile_binary_path(version: &String) -> PathBuf {
    cairo1_package_dir(version).join("cairo/bin/starknet-compile")
}

/// Path to starknet-sierra-compile binary, of the specified version.
fn starknet_sierra_compile_binary_path(version: &String) -> PathBuf {
    cairo1_package_dir(version).join("cairo/bin/starknet-sierra-compile")
}

/// Returns the path to the allowed_libfuncs.json file.
pub fn allowed_libfuncs_json_path() -> String {
    resolve_project_relative_path("crates/apollo_compile_to_casm/src/allowed_libfuncs.json")
        .unwrap()
        .to_string_lossy()
        .to_string()
}

/// Returns the path to a legacy-format allowed_libfuncs.json file (array of strings).
///
/// Older compiler versions (e.g. v2.1.0, v2.7.0) cannot parse the new map-based format
/// introduced in the main allowed_libfuncs.json. This function generates a compatibility file
/// with the old `{"allowed_libfuncs": ["name1", ...]}` format. The file is cached and only
/// regenerated when the source file is newer.
pub fn allowed_libfuncs_legacy_json_path() -> String {
    let output_dir = project_path().unwrap().join("target/tmp");
    let output_path = output_dir.join("allowed_libfuncs_legacy.json");
    let new_format_path = allowed_libfuncs_json_path();

    // Skip generation if the cached file is up-to-date with the source.
    if is_up_to_date(&output_path, &new_format_path) {
        return output_path.to_string_lossy().to_string();
    }

    // Read the new-format file and convert to old format.
    let contents = std::fs::read_to_string(&new_format_path)
        .unwrap_or_else(|err| panic!("Failed to read {new_format_path}: {err}"));
    let parsed: serde_json::Value = serde_json::from_str(&contents)
        .unwrap_or_else(|err| panic!("Failed to parse {new_format_path}: {err}"));
    let libfuncs_map = parsed["allowed_libfuncs"]
        .as_object()
        .unwrap_or_else(|| panic!("Expected 'allowed_libfuncs' to be a map in {new_format_path}"));
    let keys: Vec<&str> = libfuncs_map.keys().map(|k| k.as_str()).collect();
    let legacy_json = serde_json::json!({"allowed_libfuncs": keys}).to_string();

    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|err| panic!("Failed to create {}: {err}", output_dir.display()));

    // Write to a temp file in the same directory, then atomically rename so concurrent
    // readers never see a partially-written file.
    let mut temp_file = NamedTempFile::new_in(&output_dir).unwrap_or_else(|err| {
        panic!("Failed to create temp file in {}: {err}", output_dir.display())
    });
    temp_file
        .write_all(legacy_json.as_bytes())
        .unwrap_or_else(|err| panic!("Failed to write temp file: {err}"));
    temp_file
        .persist(&output_path)
        .unwrap_or_else(|err| panic!("Failed to persist {}: {err}", output_path.display()));

    output_path.to_string_lossy().to_string()
}

/// Returns true if `output` exists and is at least as recent as `source`.
fn is_up_to_date(output: &std::path::Path, source: &str) -> bool {
    let Ok(output_meta) = std::fs::metadata(output) else {
        return false;
    };
    let Ok(source_meta) = std::fs::metadata(source) else {
        return false;
    };
    match (output_meta.modified(), source_meta.modified()) {
        (Ok(output_mtime), Ok(source_mtime)) => output_mtime >= source_mtime,
        _ => false,
    }
}

/// Downloads the cairo package to the local directory.
/// Creates the directory if it does not exist.
async fn download_cairo_package(version: &String) {
    let directory = cairo1_package_dir(version);
    info!("Downloading Cairo package to {directory:?}.");
    std::fs::create_dir_all(&directory).unwrap();

    // Download the artifact.
    let filename = "release-x86_64-unknown-linux-musl.tar.gz";
    let package_url =
        format!("https://github.com/starkware-libs/cairo/releases/download/v{version}/{filename}");
    let curl_result = run_and_verify_output(Command::new("curl").args(["-L", &package_url]));
    let mut tar_command = Command::new("tar")
        .args(["-xz", "-C", directory.to_str().unwrap()])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let tar_command_stdin = tar_command.stdin.as_mut().unwrap();
    tar_command_stdin.write_all(&curl_result.stdout).unwrap();
    let output = tar_command.wait_with_output().unwrap();
    if !output.status.success() {
        let stderr_output = String::from_utf8(output.stderr).unwrap();
        panic!("{stderr_output}");
    }
    info!("Done.");
}

fn cairo1_package_exists(version: &String) -> bool {
    let cairo_compiler_path = starknet_compile_binary_path(version);
    let sierra_compiler_path = starknet_sierra_compile_binary_path(version);
    cairo_compiler_path.exists() && sierra_compiler_path.exists()
}

/// Verifies that the Cairo1 package (of the given version) is available.
/// Attempts to download it if not.
pub async fn verify_cairo1_package(version: &String) {
    if !cairo1_package_exists(version) {
        download_cairo_package(version).await;
    }
    assert!(cairo1_package_exists(version));
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
pub fn cairo0_compile(
    path: String,
    extra_arg: Option<String>,
    debug_info: bool,
) -> CompilationArtifacts {
    let script_type = Cairo0Script::StarknetCompileDeprecated;
    verify_cairo0_compiler_deps(&script_type);
    let mut command = Command::new(script_type.script_name());
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
    CompilationArtifacts::Cairo0 { casm: compile_output.stdout }
}

pub enum LibfuncArg {
    ListName(String),
    ListFile(String),
}

impl LibfuncArg {
    pub fn add_to_command<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        match self {
            Self::ListName(name) => command.args(["--allowed-libfuncs-list-name", name]),
            Self::ListFile(file) => command.args(["--allowed-libfuncs-list-file", file]),
        }
    }
}

/// Compiles a Cairo1 program using the compiler version set in the Cargo.toml.
pub fn cairo1_compile(
    path: String,
    version: String,
    libfunc_list_arg: LibfuncArg,
) -> CompilationArtifacts {
    assert!(cairo1_package_exists(&version));

    let sierra_output = starknet_compile(path, &version, &libfunc_list_arg);

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(&sierra_output).unwrap();
    let temp_path_str = temp_file.into_temp_path();

    // Sierra -> CASM.
    let casm_output = starknet_sierra_compile(
        temp_path_str.to_str().unwrap().to_string(),
        &version,
        &libfunc_list_arg,
    );

    CompilationArtifacts::Cairo1 { casm: casm_output, sierra: sierra_output }
}

/// Compiles Cairo1 contracts into their Sierra version using the given compiler version.
/// Assumes the relevant compiler version was already downloaded.
pub fn starknet_compile(path: String, version: &String, libfunc_list_arg: &LibfuncArg) -> Vec<u8> {
    let mut starknet_compile_commmand = Command::new(starknet_compile_binary_path(version));
    starknet_compile_commmand.args(["--single-file", &path]);
    libfunc_list_arg.add_to_command(&mut starknet_compile_commmand);
    let sierra_output = run_and_verify_output(&mut starknet_compile_commmand);

    sierra_output.stdout
}

/// Compiles Sierra code into CASM using the given compiler version.
/// Assumes the relevant compiler version was already downloaded.
fn starknet_sierra_compile(
    path: String,
    version: &String,
    libfunc_list_arg: &LibfuncArg,
) -> Vec<u8> {
    let mut sierra_compile_command = Command::new(starknet_sierra_compile_binary_path(version));
    sierra_compile_command.args([&path]);
    libfunc_list_arg.add_to_command(&mut sierra_compile_command);
    let casm_output = run_and_verify_output(&mut sierra_compile_command);
    casm_output.stdout
}
