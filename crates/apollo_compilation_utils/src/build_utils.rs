use std::process::Command;

/// Verifies that a compiler binary is installed and has the required version.
/// Panics with installation instructions if the binary is missing or has the wrong version.
pub fn verify_compiler_binary(binary_name: &str, required_version: &str) {
    match Command::new(binary_name).arg("--version").output() {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if !version_str.contains(required_version) {
                panic!(
                    "{binary_name} version {required_version} is required, but found: \
                     {version_str}. Run 'scripts/install_cargo_tools.sh' to install the correct \
                     version."
                );
            }
        }
        Err(_) => {
            panic!("{binary_name} not found. Run 'scripts/install_cargo_tools.sh' to install it.");
        }
    }
}
