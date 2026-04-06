use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::paths::{legacy_binary_path, shared_folder_dir};

pub fn install_compiler_binary(
    binary_name: &str,
    required_version: &str,
    cargo_install_args: &[&str],
    out_dir: &std::path::Path,
) {
    let binary_path = legacy_binary_path(out_dir, binary_name);
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
pub fn verify_compiler_binary(binary_path: &Path, required_version: &str) {
    let binary_name = binary_path.display();
    let install_instructions =
        "Run 'scripts/install_compiler_binaries.sh' to install the correct version.";
    match Command::new(binary_path).arg("--version").output() {
        Ok(output) => {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Extract the version token (e.g. "2.17.0-rc.4") from output like
            // "starknet-sierra-compile 2.17.0-rc.4". Using exact token match avoids
            // false positives (e.g. "1.0.1" matching "1.0.10").
            let installed_version = version_output
                .split_whitespace()
                .find(|token| token.contains('.'))
                .unwrap_or("");
            if installed_version != required_version {
                panic!(
                    "{binary_name} version {required_version} is required, but found: \
                     {installed_version}. {install_instructions}"
                );
            }
        }
        Err(_) => {
            panic!("{binary_name} not found. {install_instructions}");
        }
    }
}
