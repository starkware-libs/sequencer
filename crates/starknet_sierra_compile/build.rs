use std::process::Command;

include!("src/build_utils.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    install_starknet_sierra_compile();
    #[cfg(feature = "cairo_native")]
    install_starknet_native_compile();
}

const REQUIRED_CAIRO_LANG_VERSION: &str = "2.7.1";
#[cfg(feature = "cairo_native")]
const REQUIRED_NATIVE_VERSION: &str = "0.2.0-alpha.4";

/// Downloads the Cairo crate from StarkWare's release page and extracts its contents into the
/// `target` directory. This crate includes the `starknet-sierra-compile` binary, which is used to
/// compile Sierra to Casm. The binary is executed as a subprocess whenever Sierra compilation is
/// required.
fn install_starknet_sierra_compile() {
    let binary_name = CAIRO_LANG_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_LANG_VERSION;
    let cargo_install_args = &[CAIRO_LANG_BINARY_NAME, "--version", REQUIRED_CAIRO_LANG_VERSION];
    install_compiler_binary(binary_name, required_version, cargo_install_args);
}

/// Installs the `starknet-native-compile` crate from the current repository and moves the binary
/// to the shared executables folder. This crate includes the `starknet-native-compile` binary,
/// which is used to compile Sierra to 0x86. The binary is executed as a subprocess whenever Sierra
/// compilation is required.
#[cfg(feature = "cairo_native")]
fn install_starknet_native_compile() {
    let binary_name = NATIVE_BINARY_NAME;
    let required_version = REQUIRED_NATIVE_VERSION;

    let starknet_native_compile_crate_path = PathBuf::from("../bin").join(NATIVE_BINARY_NAME);
    let cargo_install_args = &[
        NATIVE_BINARY_NAME,
        "--path",
        starknet_native_compile_crate_path
            .to_str()
            .expect("Failed to convert the crate path to str"),
    ];
    install_compiler_binary(binary_name, required_version, cargo_install_args);
}

fn install_compiler_binary(binary_name: &str, required_version: &str, cargo_install_args: &[&str]) {
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

    let out_dir = out_dir();
    let temp_cargo_path = out_dir.join("cargo");
    let post_install_file_path = temp_cargo_path.join("bin").join(binary_name);

    // Create the temporary cargo directory if it doesn't exist
    std::fs::create_dir_all(&temp_cargo_path).expect("Failed to create cargo directory");
    let install_command_status = Command::new("cargo")
        .args([
            "install",
            "--root",
            temp_cargo_path.to_str().expect("Failed to convert cargo_path to str"),
        ])
        .args(cargo_install_args)
        .status()
        .unwrap_or_else(|_| panic!("Failed to install {binary_name}"));

    if !install_command_status.success() {
        panic!("Failed to install {}", binary_name);
    }

    // Move the 'starknet-sierra-compile' executable to a shared location
    std::fs::create_dir_all(shared_folder_dir())
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
