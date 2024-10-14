use std::process::Command;

include!("src/build_utils.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    install_native_contract_compile();
}

const REQUIRED_VERSION: &str = "0.2.0";

/// Downloads the Cairo crate from StarkWare's release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. The binary is executed as a subprocess whenever Sierra compilation is
/// required.
fn install_native_contract_compile() {
    let binary_path = binary_path();
    println!("cargo:warning=Binary path: {:?}", binary_path);

    match Command::new(&binary_path).args(["--version"]).output() {
        Ok(binary_version) => {
            let binary_version = String::from_utf8(binary_version.stdout)
                .expect("Failed to convert the binary version to a string.");
            println!("cargo:warning=binary_version = {:?}", binary_version);
            if binary_version.contains(REQUIRED_VERSION) {
                println!("cargo:warning=The {BINARY_NAME} binary is up to date.");
                return;
            } else {
                println!(
                    "The {BINARY_NAME} binary is not up to date. Installing the \
                     required version."
                );
                std::fs::remove_file(&binary_path).expect("Failed to remove the old binary.");
            }
        }
        Err(_) => {
            // FIXME:
            println!("cargo:warning=The {BINARY_NAME} binary is not installed. Installing the required version.");
            println!(
                "The {BINARY_NAME} binary is not installed. Installing the required \
                 version."
            );
        }
    }

    let out_dir = out_dir();
    let temp_cargo_path = out_dir.join("cargo");
    let post_install_file_path = temp_cargo_path.join("bin").join(BINARY_NAME);

    // Create the temporary cargo directory if it doesn't exist
    std::fs::create_dir_all(&temp_cargo_path).expect("Failed to create cargo directory");
    println!("cargo:warning=Created temp cargo directory {}.", temp_cargo_path.to_str().unwrap());
    let install_command_status = Command::new("cargo")
        .args([
            "install",
            "--path",
            "../bin/cairo-native-compile",
            "--root",
            temp_cargo_path.to_str().expect("Failed to convert cargo_path to str"),
            // "--version",
            // REQUIRED_VERSION,
        ])
        .status()
        .expect("Failed to install starknet-native-compile");

    println!("cargo:warning=Installed.");

    if !install_command_status.success() {
        panic!("Failed to install starknet-native-compile");
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

    println!("cargo:warning=Successfully set executable file: {:?}", binary_path.display());
}
