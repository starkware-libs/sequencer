use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use apollo_infra_utils::cairo0_compiler::{verify_cairo0_compiler_deps, Cairo0Script};
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_infra_utils::path::project_path;
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

/// Compiles a Cairo1 program using the compiler version set in the Cargo.toml.
pub fn cairo1_compile(path: String, version: String) -> CompilationArtifacts {
    assert!(cairo1_package_exists(&version));

    let sierra_output = starknet_compile(path, &version);

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(&sierra_output).unwrap();
    let temp_path_str = temp_file.into_temp_path();

    // Sierra -> CASM.
    let casm_output =
        starknet_sierra_compile(temp_path_str.to_str().unwrap().to_string(), &version);

    CompilationArtifacts::Cairo1 { casm: casm_output, sierra: sierra_output }
}

/// Compiles Cairo1 contracts into their Sierra version using the given compiler version.
/// Assumes the relevant compiler version was already downloaded.
pub fn starknet_compile(path: String, version: &String) -> Vec<u8> {
    let mut starknet_compile_commmand = Command::new(starknet_compile_binary_path(version));
    starknet_compile_commmand.args(["--single-file", &path, "--allowed-libfuncs-list-name", "all"]);
    let sierra_output = run_and_verify_output(&mut starknet_compile_commmand);

    sierra_output.stdout
}

/// Compiles Sierra code into CASM using the given compiler version.
/// Assumes the relevant compiler version was already downloaded.
fn starknet_sierra_compile(path: String, version: &String) -> Vec<u8> {
    let mut sierra_compile_command = Command::new(starknet_sierra_compile_binary_path(version));
    sierra_compile_command.args([&path, "--allowed-libfuncs-list-name", "all"]);
    let casm_output = run_and_verify_output(&mut sierra_compile_command);
    casm_output.stdout
}
