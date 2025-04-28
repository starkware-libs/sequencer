use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;
use std::{env, fs};

use apollo_infra_utils::cairo_compiler_version::cairo1_compiler_version;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use apollo_infra_utils::path::{project_path, resolve_project_relative_path};
use tempfile::NamedTempFile;
use tracing::info;

static CAIRO0_PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());
const CAIRO1_REPO_RELATIVE_PATH_OVERRIDE_ENV_VAR: &str = "CAIRO1_REPO_RELATIVE_PATH";
const DEFAULT_CAIRO1_REPO_RELATIVE_PATH: &str = "../../../cairo";

pub enum CompilationArtifacts {
    Cairo0 { casm: Vec<u8> },
    Cairo1 { casm: Vec<u8>, sierra: Vec<u8> },
}

pub fn cairo1_compiler_tag() -> String {
    format!("v{}", cairo1_compiler_version())
}

/// Returns the path to the local Cairo1 compiler repository.
/// Returns <sequencer_repo_root>/<RELATIVE_PATH_TO_CAIRO_REPO>, where the relative path can be
/// overridden by the environment variable (otherwise, the default is used).
fn local_cairo1_compiler_repo_path() -> PathBuf {
    // Location of blockifier's Cargo.toml.
    let manifest_dir = compile_time_cargo_manifest_dir!();

    Path::new(&manifest_dir).join(
        env::var(CAIRO1_REPO_RELATIVE_PATH_OVERRIDE_ENV_VAR)
            .unwrap_or_else(|_| DEFAULT_CAIRO1_REPO_RELATIVE_PATH.into()),
    )
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
    CompilationArtifacts::Cairo0 { casm: compile_output.stdout }
}

/// Compiles a Cairo1 program using the compiler version set in the Cargo.toml.
pub fn cairo1_compile(
    path: String,
    git_tag_override: Option<String>,
    _cargo_nightly_arg: Option<String>,
) -> CompilationArtifacts {
    let (tag, _cairo_repo_path) = get_tag_and_repo_file_path(git_tag_override);
    let version = tag.strip_prefix("v").unwrap().to_string();
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

/// Verifies that the required dependencies are available before compiling; panics if unavailable.
fn verify_cairo0_compiler_deps() {
    // Python compiler. Verify correct version.
    let cairo_lang_version_output =
        Command::new("sh").arg("-c").arg("pip freeze | grep cairo-lang").output().unwrap().stdout;
    let cairo_lang_version_untrimmed = String::from_utf8(cairo_lang_version_output).unwrap();
    let cairo_lang_version =
        cairo_lang_version_untrimmed.trim().split("==").nth(1).unwrap_or_else(|| {
            panic!("Unexpected cairo-lang version format '{cairo_lang_version_untrimmed}'.")
        });
    let requirements_contents = fs::read_to_string(&*CAIRO0_PIP_REQUIREMENTS_FILE).unwrap();
    let expected_cairo_lang_version = requirements_contents
        .lines()
        .find(|line| line.starts_with("cairo-lang"))
        .unwrap_or_else(|| {
            panic!("Could not find cairo-lang in {:?}.", *CAIRO0_PIP_REQUIREMENTS_FILE)
        })
        .trim()
        .split("==")
        .nth(1)
        .unwrap_or_else(|| {
            panic!(
                "Malformed cairo-lang dependency (expected 'cairo-lang==X') in {:?}.",
                *CAIRO0_PIP_REQUIREMENTS_FILE
            )
        });

    assert_eq!(
        expected_cairo_lang_version, cairo_lang_version,
        "cairo-lang version {expected_cairo_lang_version} not found (installed version: \
         {cairo_lang_version}). Please run:\npip3.9 install -r {:?}\nthen rerun the test.",
        *CAIRO0_PIP_REQUIREMENTS_FILE
    );
}

fn get_tag_and_repo_file_path(git_tag_override: Option<String>) -> (String, PathBuf) {
    let tag = git_tag_override.unwrap_or(cairo1_compiler_tag());
    let cairo_repo_path = local_cairo1_compiler_repo_path();
    // Check if the path is a directory.
    assert!(
        cairo_repo_path.is_dir(),
        "Cannot verify Cairo1 contracts, Cairo repo not found at {0}.\nPlease run:\n\
        git clone https://github.com/starkware-libs/cairo {0}\nThen rerun the test.",
        cairo_repo_path.to_string_lossy(),
    );

    (tag, cairo_repo_path)
}
