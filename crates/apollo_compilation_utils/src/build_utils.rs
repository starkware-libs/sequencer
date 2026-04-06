use std::path::Path;
use std::process::Command;

/// Verifies that a compiler binary is installed and has the required version.
/// Panics with installation instructions if the binary is missing or has the wrong version.
///
/// Expects `--version` output in the form `<binary-name> <version>` (with optional
/// trailing tokens). The match is anchored to the binary's own file name so that
/// a misbehaving or substituted binary printing a stray dotted token elsewhere in
/// its output cannot satisfy the check.
pub fn verify_compiler_binary(binary_path: &Path, required_version: &str) {
    let binary_name = binary_path.display();
    let expected_prefix = binary_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| panic!("{binary_name} has no UTF-8 file name."));
    let install_instructions =
        "Run 'scripts/install_compiler_binaries.sh' to install the correct version.";
    match Command::new(binary_path).arg("--version").output() {
        Ok(output) => {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // Expect "<binary-name> <version>" on the first non-empty line. Anchoring
            // to <binary-name> avoids both substring false-positives ("1.0.1" vs
            // "1.0.10") and adversarial padding (e.g. a banner that includes the
            // required version elsewhere in stdout).
            let first_line = version_output.lines().find(|line| !line.trim().is_empty());
            let installed_version = first_line
                .and_then(|line| line.strip_prefix(expected_prefix))
                .map(|rest| rest.trim_start())
                .and_then(|rest| rest.split_whitespace().next())
                .unwrap_or("");
            if installed_version != required_version {
                panic!(
                    "{binary_name} version {required_version} is required, but found: \
                     {installed_version:?} (raw output: {version_output:?}). \
                     {install_instructions}"
                );
            }
        }
        Err(_) => {
            panic!("{binary_name} not found. {install_instructions}");
        }
    }
}
