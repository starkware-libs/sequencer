use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;

include!("src/constants.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Skip validation if explicitly disabled (useful for CI/Docker builds)
    if std::env::var("SKIP_SIERRA_COMPILE_VALIDATION").is_ok() {
        println!("⚠️  Skipping starknet-sierra-compile validation (SKIP_SIERRA_COMPILE_VALIDATION set)");
        return;
    }

    validate_starknet_sierra_compile();
}

/// Validates that the `starknet-sierra-compile` binary is available with the correct version.
/// This binary is used to compile Sierra to Casm as a subprocess.
fn validate_starknet_sierra_compile() {
    let binary_name = CAIRO_LANG_BINARY_NAME;
    let required_version = CAIRO1_COMPILER_VERSION;

    // Check if binary exists in PATH
    if let Some(binary_path) = check_binary_in_path(binary_name, required_version) {
        println!("✅ Found {binary_name} v{required_version} at: {}", binary_path.display());
        return;
    }

    // If not in PATH, provide helpful error message
    eprintln!(
        "\n❌ ERROR: Required binary '{binary_name}' version '{required_version}' not found!"
    );
    eprintln!("\nTo install it, run:");
    eprintln!("  cargo install {binary_name} --version {required_version} --locked");
    eprintln!(
        "\nOr in CI, ensure the binary is built and available in PATH before building this crate."
    );
    eprintln!("The binary will be used as a subprocess for Sierra → CASM compilation.\n");
    panic!("Missing required compiler binary: {binary_name} v{required_version}");
}

/// Check if a binary exists in PATH with the correct version
fn check_binary_in_path(binary_name: &str, required_version: &str) -> Option<std::path::PathBuf> {
    use std::process::Command;

    // Check if binary exists in PATH
    let which_output = Command::new("which").arg(binary_name).output().ok()?;
    if !which_output.status.success() {
        return None;
    }

    let binary_path = String::from_utf8(which_output.stdout).ok()?;
    let binary_path = binary_path.trim();
    let binary_path = std::path::PathBuf::from(binary_path);

    // Check if it has the correct version
    let version_output = Command::new(&binary_path).args(["--version"]).output().ok()?;
    if !version_output.status.success() {
        return None;
    }

    let version_string = String::from_utf8(version_output.stdout).ok()?;
    if version_string.contains(required_version) {
        Some(binary_path)
    } else {
        eprintln!(
            "⚠️  Found {binary_name} in PATH but wrong version. Expected: {required_version}, \
             found: {}",
            version_string.trim()
        );
        None
    }
}
