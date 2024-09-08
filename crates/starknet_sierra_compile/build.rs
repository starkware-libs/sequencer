use std::process::Command;

include!("src/build_utils.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    install_starknet_sierra_compile();
}

const REQUIRED_VERSION: &str = "2.7.1";

/// Downloads the Cairo crate from StarkWare's release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. The binary is executed as a subprocess whenever Sierra compilation is
/// required.
fn install_starknet_sierra_compile() {
    let binary_path = binary_path();

    match Command::new(&binary_path).args(["--version"]).output() {
        Ok(binary_version) => {
            let binary_version = String::from_utf8(binary_version.stdout)
                .expect("Failed to convert the binary version to a string.");
            if binary_version.contains(REQUIRED_VERSION) {
                println!("The starknet-sierra-compile binary is up to date.");
                return;
            } else {
                println!(
                    "The starknet-sierra-compile binary is not up to date. Installing the \
                     required version."
                );
                std::fs::remove_file(&binary_path).expect("Failed to remove the old binary.");
            }
        }
        Err(_) => {
            println!(
                "The starknet-sierra-compile binary is not installed. Installing the required \
                 version."
            );
        }
    }

    let out_dir = out_dir();
    let temp_cargo_path = out_dir.join("cargo");
    let post_install_file_path = temp_cargo_path.join("bin").join(BINARY_NAME);

    // Create the temporary cargo directory if it doesn't exist
    std::fs::create_dir_all(&temp_cargo_path).expect("Failed to create cargo directory");
    let install_command_status = Command::new("cargo")
        .args([
            "install",
            BINARY_NAME,
            "--root",
            temp_cargo_path.to_str().expect("Failed to convert cargo_path to str"),
            "--version",
            REQUIRED_VERSION,
        ])
        .status()
        .expect("Failed to install starknet-sierra-compile");

    if !install_command_status.success() {
        panic!("Failed to install starknet-sierra-compile");
    }

    // Move the 'starknet-sierra-compile' executable to a shared location
    std::fs::create_dir_all(shared_folder_dir())
        .expect("Failed to create shared executables folder");
    let move_command_status = Command::new("mv")
        .args([post_install_file_path.as_os_str(), binary_path.as_os_str()])
        .status()
        .expect("Failed to perform mv command.");

    if !move_command_status.success() {
        panic!("Failed to move the starknet-sierra-compile binary to the shared folder.");
    }

    std::fs::remove_dir_all(temp_cargo_path).expect("Failed to remove the cargo directory.");

    println!("Successfully set executable file: {:?}", binary_path.display());
}
