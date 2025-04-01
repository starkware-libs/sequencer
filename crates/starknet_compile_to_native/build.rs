#[cfg(feature = "cairo_native")]
use std::process::Command;

#[cfg(feature = "cairo_native")]
use tempfile::TempDir;

include!("src/constants.rs");
include!("src/paths.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    set_run_time_out_dir_env_var();
    #[cfg(feature = "cairo_native")]
    install_starknet_native_compile();
}

/// Install the `starknet-native-compile` binary from the Cairo Native crate and moves the binary
/// to the `target` directory. The `starknet-native-compile` binary is used to compile Sierra to
/// Native. The binary is executed as a subprocess whenever Sierra to Cairo compilation is required.
#[cfg(feature = "cairo_native")]
fn install_starknet_native_compile() {
    let binary_name = CAIRO_NATIVE_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_NATIVE_VERSION;

    let cargo_install_args = &["cairo-native", "--version", required_version, "--bin", binary_name];
    install_compiler_binary(binary_name, required_version, cargo_install_args);
}

#[cfg(feature = "cairo_native")]
fn install_compiler_binary(binary_name: &str, required_version: &str, cargo_install_args: &[&str]) {
    let binary_path = binary_path(out_dir(), binary_name);
    println!("cargo:rerun-if-changed={}", binary_path.to_str().unwrap());

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
        panic!("Failed to install {}", binary_name);
    }

    // Move the '{binary_name}' executable to a shared location.
    std::fs::create_dir_all(shared_folder_dir(out_dir()))
        .expect("Failed to create shared executables folder");
    let move_command_status = Command::new("mv")
        .args([post_install_file_path.as_os_str(), binary_path.as_os_str()])
        .status()
        .expect("Failed to perform mv command.");

    if !move_command_status.success() {
        panic!("Failed to move the {} binary to the shared folder.", binary_name);
    }

    std::fs::remove_dir_all(temp_cargo_path).expect("Failed to remove the cargo directory.");

    println!("Successfully set executable file: {:?}", binary_path.display());
}

// Sets the `RUNTIME_ACCESSIBLE_OUT_DIR` environment variable to the `OUT_DIR` value, which will be
// available only after the build is completed. Most importantly, it is available during runtime.
fn set_run_time_out_dir_env_var() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set");
    println!("cargo:rustc-env=RUNTIME_ACCESSIBLE_OUT_DIR={}", out_dir);
}

#[cfg(feature = "cairo_native")]
// Returns the OUT_DIR. This function is only operable at build time.
fn out_dir() -> std::path::PathBuf {
    std::env::var("OUT_DIR")
        .expect("Failed to get the build time OUT_DIR environment variable")
        .into()
}
