use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::paths::{binary_path, shared_folder_dir};

pub fn install_compiler_binary(
    binary_name: &str,
    required_version: &str,
    cargo_install_args: &[&str],
    out_dir: &std::path::Path,
) {
    let binary_path = binary_path(out_dir, binary_name);
    match Command::new(&binary_path).args(["--version"]).output() {
        Ok(binary_version) => {
            let binary_version = String::from_utf8(binary_version.stdout)
                .expect("Failed to convert the binary version to a string.");
            if binary_version.contains(required_version) {
                println!("The {binary_name} binary is up to date.");
                return;
            } else {
                println!(
                    "The {binary_name} binary is not up to date. Installing the required version."
                );
                std::fs::remove_file(&binary_path).expect("Failed to remove the old binary.");
            }
        }
        Err(_) => {
            println!("The {binary_name} binary is not installed. Installing the required version.");
        }
    }

    let temp_cargo_path = TempDir::new().expect("Failed to create a temporary directory.");
    let post_install_file_path = temp_cargo_path.path().join("bin").join(binary_name);

    let install_command_status = Command::new("cargo")
        .args([
            "install",
            "--root",
            temp_cargo_path.path().to_str().expect("Failed to convert cargo_path to str"),
            "--locked",
        ])
        .args(cargo_install_args)
        .status()
        .unwrap_or_else(|_| panic!("Failed to install {binary_name}"));

    if !install_command_status.success() {
        panic!("Failed to install {binary_name}");
    }

    // Move the '{binary_name}' executable to a shared location.
    std::fs::create_dir_all(shared_folder_dir(out_dir))
        .expect("Failed to create shared executables folder");
    let move_command_status = Command::new("mv")
        .args([post_install_file_path.as_os_str(), binary_path.as_os_str()])
        .status()
        .expect("Failed to perform mv command.");

    if !move_command_status.success() {
        panic!("Failed to move the {binary_name} binary to the shared folder.");
    }

    std::fs::remove_dir_all(temp_cargo_path).expect("Failed to remove the cargo directory.");

    println!("Successfully set executable file: {:?}", binary_path.display());
}

/// Verifies that a compiler binary is installed and has the required version.
/// Panics with installation instructions if the binary is missing or has the wrong version.
///
/// Expects `--version` output in the form `<binary-name> <version>` (with optional
/// trailing tokens). The match is anchored to the binary's own file name so that
/// a misbehaving or substituted binary printing a stray dotted token elsewhere in
/// its output cannot satisfy the check.
pub fn verify_compiler_binary(binary_path: &Path, required_version: &str) {
    let binary_name = binary_path.display();
    let expected_prefix = binary_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("{binary_name} has no UTF-8 file name."));
    let install_instructions =
        "Run 'scripts/install_compiler_binaries.sh' to install the correct version.";
    match Command::new(binary_path).arg("--version").output() {
        Ok(output) => {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Expect "<binary-name> <version>" on the first non-empty line. Anchoring
            // to <binary-name> avoids both substring false-positives ("1.0.1" vs
            // "1.0.10") and adversarial padding (e.g. a banner that includes the
            // required version elsewhere in stdout).
            let first_line = version_output.lines().find(|line| !line.trim().is_empty());
            let installed_version = first_line
                .and_then(|line| line.strip_prefix(expected_prefix))
                .map(|rest| rest.trim_start())
                .and_then(|rest| rest.split_whitespace().next())
                .unwrap_or("");
            if installed_version != required_version {
                panic!(
                    "{binary_name} version {required_version} is required, but found: \
                     {installed_version:?} (raw output: {version_output:?}). \
                     {install_instructions}"
                );
            }
        }
        Err(_) => {
            panic!("{binary_name} not found. {install_instructions}");
        }
    }
}
