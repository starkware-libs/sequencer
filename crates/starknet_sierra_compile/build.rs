use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=../../Cargo.lock");
    println!("cargo::rerun-if-changed=build.rs");

    install_starknet_sierra_compile();
}

fn install_starknet_sierra_compile() {
    let binary_name = "starknet-sierra-compile";
    let required_version = "2.7.1";

    // Check if the binary is installed and whether it's the correct version
    let check_version = Command::new(binary_name).arg("--version").output();

    if let Ok(output) = check_version {
        if output.status.success() {
            let installed_version = String::from_utf8(output.stdout)
                .expect("Failed to read the version of starknet-sierra-compile");

            if installed_version.trim() == format!("{binary_name} v{required_version}") {
                println!("{} is already installed and up-to-date.", binary_name);
                return;
            }
        }
    }

    // Install the binary locally
    println!("Installing {} version {}...", binary_name, required_version);

    let status = Command::new("cargo")
        .args(["install", binary_name, "--version", required_version])
        .status()
        .expect("Failed to execute cargo install for starknet-sierra-compile");

    if !status.success() {
        panic!("Failed to install starknet-sierra-compile");
    }

    println!("Successfully installed {} version {}.", binary_name, required_version);
}
