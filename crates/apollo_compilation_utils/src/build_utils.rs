use std::path::Path;
use std::process::Command;

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
            let installed_version =
                version_output.split_whitespace().find(|token| token.contains('.')).unwrap_or("");
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
