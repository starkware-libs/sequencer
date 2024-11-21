use std::process::Command;

use infra_utils::path::resolve_project_relative_path;
use tempfile::TempDir;

include!("src/path_constants.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    install_starknet_sierra_compile();
    #[cfg(feature = "cairo_native")]
    install_starknet_native_compile();
}

const REQUIRED_CAIRO_LANG_VERSION: &str = "2.7.1";
#[cfg(feature = "cairo_native")]
const REQUIRED_CAIRO_NATIVE_VERSION: &str = "0.2.4";

/// Downloads the Cairo crate from StarkWare's release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. The binary is executed as a subprocess whenever Sierra compilation is
/// required.
fn install_starknet_sierra_compile() {
    let binary_name = CAIRO_LANG_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_LANG_VERSION;

    let cargo_install_args = &[binary_name, "--version", required_version];
    install_compiler_binary(binary_name, required_version, cargo_install_args);
}

/// Installs the `starknet-native-compile` crate from the current repository and moves the binary
/// to the shared executables folder. This crate includes the `starknet-native-compile` binary,
/// which is used to compile Sierra to 0x86. The binary is executed as a subprocess whenever Sierra
/// compilation is required.
#[cfg(feature = "cairo_native")]
fn install_starknet_native_compile() {
    let binary_name = CAIRO_NATIVE_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_NATIVE_VERSION;

    // Set the runtime library path. This is required for Cairo native compilation.
    let runtime_library_path = resolve_project_relative_path(RUNTIME_LIBRARY_PATH).unwrap();
    println!("cargo:rustc-env=CAIRO_NATIVE_RUNTIME_LIBRARY={}", runtime_library_path.display());
    println!("cargo:rerun-if-env-changed=CAIRO_NATIVE_RUNTIME_LIBRARY");

    // Create a directory to store the compiled output.
    let compiled_output_dir = resolve_project_relative_path(NATIVE_COMPILE_OUT_DIR).unwrap();
    std::fs::create_dir_all(&compiled_output_dir).expect("Failed to create the output directory.");

    let starknet_native_compile_crate_path =
        resolve_project_relative_path(CAIRO_NATIVE_CRATE_PATH).unwrap();

    let starknet_native_compile_crate_path_str = starknet_native_compile_crate_path
        .to_str()
        .expect("Failed to convert the crate path to str");
    println!("cargo:rerun-if-changed={}", starknet_native_compile_crate_path_str);

    let cargo_install_args = &["--path", starknet_native_compile_crate_path_str];
    install_compiler_binary(binary_name, required_version, cargo_install_args);
}

fn install_compiler_binary(binary_name: &str, required_version: &str, cargo_install_args: &[&str]) {
    let binary_path =
        resolve_project_relative_path(SHARED_EXECUTABLES_DIR).unwrap().join(binary_name);
    println!("cargo:rerun-if-changed={:?}", binary_path);

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
        ])
        .args(cargo_install_args)
        .status()
        .unwrap_or_else(|_| panic!("Failed to install {binary_name}"));

    if !install_command_status.success() {
        panic!("Failed to install {}", binary_name);
    }

    // Move the executable named '{binary_name}' to a shared location.
    std::fs::create_dir_all(resolve_project_relative_path(SHARED_EXECUTABLES_DIR).unwrap())
        .expect("Failed to create shared executables folder");
    let move_command_status = Command::new("mv")
        .args([post_install_file_path.as_os_str(), binary_path.as_os_str()])
        .status()
        .expect("Failed to perform mv command.");

    if !move_command_status.success() {
        panic!("Failed to move the {} binary to the shared folder.", binary_name);
    }

    temp_cargo_path.close().expect("Failed to remove the temporary cargo directory.");

    println!("Successfully set executable file: {:?}", binary_path.display());
}
