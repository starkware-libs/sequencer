use std::fs::{self, File};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use apollo_infra_utils::cairo0_compiler::Cairo0Script;
use apollo_infra_utils::cairo0_compiler_test_utils::verify_cairo0_compiler_deps;
use apollo_infra_utils::path::{project_path, resolve_project_relative_path};
use tempfile::NamedTempFile;
use tracing::{info, warn};

#[cfg(test)]
#[path = "cairo_compile_test.rs"]
mod cairo_compile_test;

pub enum CompilationArtifacts {
    Cairo0 { casm: Vec<u8> },
    Cairo1 { casm: Vec<u8>, sierra: Vec<u8> },
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

/// Commit marker written after `tar` extraction fully completes. Required because
/// a file `.exists()` while `tar` is still writing to it, and exec'ing such a file
/// returns `ETXTBSY`.
fn cairo1_package_complete_marker(version: &String) -> PathBuf {
    cairo1_package_dir(version).join(".download_complete")
}

/// Returns the path to the allowed_libfuncs.json file.
pub fn allowed_libfuncs_json_path() -> String {
    resolve_project_relative_path("crates/apollo_compile_to_casm/resources/allowed_libfuncs.json")
        .unwrap()
        .to_string_lossy()
        .to_string()
}

/// Returns the path to the legacy-format allowed_libfuncs_legacy.json file (array of strings).
///
/// Older compiler versions (e.g. v2.1.0, v2.7.0) cannot parse the new map-based format. This
/// file is committed to the repo and kept in sync via an `expect_file!` test.
pub fn allowed_libfuncs_legacy_json_path() -> String {
    resolve_project_relative_path(
        "crates/blockifier_test_utils/resources/allowed_libfuncs_legacy.json",
    )
    .unwrap()
    .to_string_lossy()
    .to_string()
}

/// Converts the new-format allowed_libfuncs.json (map) to the legacy format (array of strings).
pub fn generate_allowed_libfuncs_legacy_json() -> String {
    let new_format_path = allowed_libfuncs_json_path();
    let contents = std::fs::read_to_string(&new_format_path)
        .unwrap_or_else(|err| panic!("Failed to read {new_format_path}: {err}"));
    let parsed: serde_json::Value = serde_json::from_str(&contents)
        .unwrap_or_else(|err| panic!("Failed to parse {new_format_path}: {err}"));
    let libfuncs_map = parsed["allowed_libfuncs"]
        .as_object()
        .unwrap_or_else(|| panic!("Expected 'allowed_libfuncs' to be a map in {new_format_path}"));
    let keys: Vec<&str> = libfuncs_map.keys().map(|k| k.as_str()).collect();
    serde_json::json!({"allowed_libfuncs": keys}).to_string()
}

/// Maximum number of attempts to download and extract the Cairo package before giving up.
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;

/// Delay before retrying a failed download, scaled by the attempt number.
const DOWNLOAD_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Seconds curl may spend establishing the connection.
const CURL_CONNECT_TIMEOUT_SECS: &str = "10";

/// curl aborts a transfer that stays below `CURL_STALL_BYTES_PER_SEC` for
/// `CURL_STALL_SECS` seconds.
const CURL_STALL_BYTES_PER_SEC: &str = "1024";
const CURL_STALL_SECS: &str = "20";

/// Exit code of `curl --fail` on an HTTP response status of 400 or above.
const CURL_HTTP_ERROR_EXIT_CODE: i32 = 22;

enum DownloadError {
    /// May succeed on retry.
    Transient(String),
    /// An HTTP 4xx response that a retry cannot fix.
    Permanent(String),
}

/// Downloads the cairo package to the local directory.
/// Creates the directory if it does not exist.
fn download_cairo_package(version: &String) {
    let directory = cairo1_package_dir(version);
    info!("Downloading Cairo package to {directory:?}.");

    let filename = "release-x86_64-unknown-linux-musl.tar.gz";
    let package_url =
        format!("https://github.com/starkware-libs/cairo/releases/download/v{version}/{filename}");

    let mut last_error = String::new();
    for attempt in 1..=MAX_DOWNLOAD_ATTEMPTS {
        // Safe because the lock file used by `with_file_lock` lives beside this directory.
        reset_directory(&directory);
        match stream_download_and_extract(&package_url, &directory) {
            Ok(()) => {
                // Written last: acts as a commit marker so concurrent callers checking
                // `cairo1_package_exists` never observe a half-extracted package.
                let marker = cairo1_package_complete_marker(version);
                fs::write(&marker, b"").unwrap_or_else(|error| {
                    panic!("Failed to write download marker {marker:?}: {error}")
                });
                info!("Done.");
                return;
            }
            Err(DownloadError::Permanent(error)) => {
                panic!("Failed to download Cairo package from {package_url}: {error}");
            }
            Err(DownloadError::Transient(error)) => {
                last_error = error;
                if attempt < MAX_DOWNLOAD_ATTEMPTS {
                    warn!(
                        "Cairo package download attempt {attempt}/{MAX_DOWNLOAD_ATTEMPTS} failed, \
                         retrying: {last_error}"
                    );
                    thread::sleep(DOWNLOAD_RETRY_BACKOFF * attempt);
                }
            }
        }
    }
    panic!(
        "Failed to download Cairo package from {package_url} after {MAX_DOWNLOAD_ATTEMPTS} \
         attempts. Last error: {last_error}"
    );
}

/// Removes `directory` if it exists and recreates it empty.
fn reset_directory(directory: &Path) {
    match fs::remove_dir_all(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => panic!("Failed to remove {directory:?}: {error}"),
    }
    fs::create_dir_all(directory)
        .unwrap_or_else(|error| panic!("Failed to create {directory:?}: {error}"));
}

/// Streams the release tarball at `package_url` directly into `tar`, extracting it into
/// `directory`, without buffering the whole archive in memory. Returns an error instead of
/// panicking so the caller can retry.
fn stream_download_and_extract(package_url: &str, directory: &Path) -> Result<(), DownloadError> {
    let mut curl_process = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            CURL_CONNECT_TIMEOUT_SECS,
            "--speed-limit",
            CURL_STALL_BYTES_PER_SEC,
            "--speed-time",
            CURL_STALL_SECS,
            package_url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| DownloadError::Transient(format!("Failed to spawn curl: {error}")))?;
    let curl_stdout = curl_process.stdout.take().unwrap();

    let mut tar_process = match Command::new("tar")
        .args(["-xz", "-C"])
        .arg(directory)
        .stdin(Stdio::from(curl_stdout))
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(tar_process) => tar_process,
        Err(error) => {
            curl_process.kill().ok();
            curl_process.wait().ok();
            return Err(DownloadError::Transient(format!("Failed to spawn tar: {error}")));
        }
    };

    // Drain both stderr pipes on background threads before waiting on either child: if a child
    // writes enough to fill its stderr pipe, waiting on it before the pipe is drained would
    // deadlock.
    let curl_stderr_reader = spawn_stderr_reader(curl_process.stderr.take().unwrap());
    let tar_stderr_reader = spawn_stderr_reader(tar_process.stderr.take().unwrap());

    let curl_wait_result = curl_process.wait();
    let tar_wait_result = tar_process.wait();
    let curl_stderr = curl_stderr_reader.join().unwrap();
    let tar_stderr = tar_stderr_reader.join().unwrap();
    let curl_status = curl_wait_result
        .map_err(|error| DownloadError::Transient(format!("Failed to wait for curl: {error}")))?;
    let tar_status = tar_wait_result
        .map_err(|error| DownloadError::Transient(format!("Failed to wait for tar: {error}")))?;

    match (curl_status.success(), tar_status.success()) {
        (true, true) => Ok(()),
        (false, tar_succeeded) => {
            let mut message = format!("curl failed: {curl_stderr}");
            if !tar_succeeded {
                message.push_str(&format!("; tar failed: {tar_stderr}"));
            }
            if curl_status.code() == Some(CURL_HTTP_ERROR_EXIT_CODE)
                && is_permanent_http_error(&curl_stderr)
            {
                Err(DownloadError::Permanent(message))
            } else {
                Err(DownloadError::Transient(message))
            }
        }
        (true, false) => Err(DownloadError::Transient(format!("tar failed: {tar_stderr}"))),
    }
}

fn spawn_stderr_reader(mut stderr_pipe: ChildStderr) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut stderr_bytes = Vec::new();
        stderr_pipe.read_to_end(&mut stderr_bytes).ok();
        String::from_utf8_lossy(&stderr_bytes).into_owned()
    })
}

/// Whether `curl --fail` stderr reports an HTTP 4xx status other than 408 (Request Timeout) and
/// 429 (Too Many Requests). curl prints the status as "The requested URL returned error: 404".
fn is_permanent_http_error(curl_stderr: &str) -> bool {
    let Some((_, after_marker)) = curl_stderr.split_once("returned error: ") else {
        return false;
    };
    let status_digits: String = after_marker.chars().take_while(char::is_ascii_digit).collect();
    match status_digits.parse::<u16>() {
        Ok(status) => (400..500).contains(&status) && status != 408 && status != 429,
        Err(_) => false,
    }
}

fn cairo1_package_exists(version: &String) -> bool {
    let cairo_compiler_path = starknet_compile_binary_path(version);
    let sierra_compiler_path = starknet_sierra_compile_binary_path(version);
    let marker_path = cairo1_package_complete_marker(version);
    cairo_compiler_path.exists() && sierra_compiler_path.exists() && marker_path.exists()
}

/// Appends `.lock` to a path, used by `with_file_lock` callers to derive a lock file path.
pub(crate) fn lock_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.display()))
}

/// Executes `action` at most once, guarded by a file lock at `lock_path`.
///
/// Uses double-checked locking: if `is_done()` returns `true` before acquiring the lock the
/// call is a no-op. After acquiring the lock `is_done()` is re-checked so that concurrent
/// callers that lost the race also skip `action`.
pub(crate) fn with_file_lock(lock_path: &Path, is_done: impl Fn() -> bool, action: impl FnOnce()) {
    if is_done() {
        return;
    }

    fs::create_dir_all(lock_path.parent().unwrap())
        .unwrap_or_else(|e| panic!("Failed to create directory for lock file: {e}"));
    let lock_file = File::create(lock_path)
        .unwrap_or_else(|e| panic!("Failed to create lock file {lock_path:?}: {e}"));
    lock_file.lock().unwrap_or_else(|e| panic!("Failed to acquire lock on {lock_path:?}: {e}"));

    if is_done() {
        return;
    }

    action();
}

/// Verifies that the Cairo1 package (of the given version) is available.
/// Attempts to download it if not. Uses a per-version file lock so concurrent callers (threads
/// or processes) never download the same package redundantly.
pub fn verify_cairo1_package(version: &String) {
    let dir = cairo1_package_dir(version);
    with_file_lock(
        &lock_path_for(&dir),
        || cairo1_package_exists(version),
        || {
            eprintln!(
                "[cairo_compile] Cairo 1 compiler v{version} not found locally, downloading..."
            );
            download_cairo_package(version);
            eprintln!("[cairo_compile] Cairo 1 compiler v{version} downloaded successfully.");
            assert!(cairo1_package_exists(version));
        },
    );
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

    /// Returns the file path if this is a `ListFile` variant.
    pub fn file_path(&self) -> &str {
        match self {
            Self::ListFile(path) => path,
            Self::ListName(_) => panic!("LibfuncArg::ListName has no file path"),
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
