use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=../../Cargo.lock");
    println!("cargo::rerun-if-changed=build.rs");

    install_starknet_sierra_compile();
}

fn install_starknet_sierra_compile() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let bin_path = Path::new(&out_dir).join("bin");

    let binary_name = "starknet-sierra-compile";

    // Create the bin directory if it doesn't exist
    std::fs::create_dir_all(bin_path.clone()).expect("Failed to create bin directory");

    // Path to the local binary
    let binary_path = bin_path.join(binary_name);
    // TODO(Arni): Add the configurable parameters to the function.
    let _starknet_sierra_compile_version: Option<String> = None;

    // Check if the binary is already installed locally
    if !binary_path.exists() {
        // Install the binary locally

        let status = Command::new("cargo")
            .args({
                let args = vec![
                    "install",
                    "starknet-sierra-compile",
                    "--root",
                    bin_path.to_str().expect("Failed to convert bin_path to str"),
                ];

                args
            })
            .status()
            .expect("Failed to install starknet-sierra-compile");

        if !status.success() {
            panic!("Failed to install starknet-sierra-compile");
        }
    }

    // Print the path to the installed binary so that it can be used in the main application
    println!("cargo::rustc-env=STARKNET_SIERRA_COMPILE_BIN={}", binary_path.display());
}
