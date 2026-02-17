use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::paths::{binary_path, shared_folder_dir};

pub fn install_compiler_binary(
    binary_name: &str,
    required_version: &str,
    cargo_install_args: &[&str],
) {
    let binary_path = binary_path(binary_name);
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
    std::fs::create_dir_all(shared_folder_dir())
        .expect("Failed to create shared executables folder");
    move_binary(&post_install_file_path, &binary_path);

    println!("Successfully set executable file: {:?}", binary_path.display());
}

fn move_binary(source: &Path, target: &Path) {
    match std::fs::rename(source, target) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::CrossesDevices => {
            std::fs::copy(source, target).expect("Failed to copy the binary to the shared folder.");
            std::fs::remove_file(source)
                .expect("Failed to remove the temporary binary after copy.");
        }
        Err(error) => {
            panic!("Failed to move binary to shared folder: {error}");
        }
    }
}
