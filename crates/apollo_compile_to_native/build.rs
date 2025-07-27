include!("src/constants.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Skip validation if explicitly disabled (useful for CI/Docker builds)
    if std::env::var("SKIP_NATIVE_COMPILE_VALIDATION").is_ok() {
        println!("⚠️  Skipping starknet-native-compile validation (SKIP_NATIVE_COMPILE_VALIDATION set)");
        return;
    }

    validate_starknet_native_compile();
}

/// Validates that the `starknet-native-compile` binary is available with the correct version.
/// This binary is used to compile Sierra to Cairo Native as a subprocess.
fn validate_starknet_native_compile() {
    let binary_name = CAIRO_NATIVE_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_NATIVE_VERSION;

    // Check if binary exists in PATH
    if let Some(binary_path) = check_binary_in_path(binary_name, required_version) {
        println!("✅ Found {binary_name} v{required_version} at: {}", binary_path.display());
        return;
    }

    // If not in PATH, provide helpful error message with LLVM requirements
    eprintln!(
        "\n❌ ERROR: Required binary '{binary_name}' version '{required_version}' not found!"
    );
    eprintln!("\n🔧 To install it:");
    eprintln!("  1. First ensure LLVM 19 is installed:");
    eprintln!("     sudo apt update && sudo apt install llvm-19-dev libmlir-19-dev");
    eprintln!("     # OR run: sudo ./scripts/dependencies.sh");
    eprintln!();
    eprintln!("  2. Set required environment variables:");
    eprintln!("     export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19");
    eprintln!("     export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19");
    eprintln!("     export TABLEGEN_190_PREFIX=/usr/lib/llvm-19");
    eprintln!();
    eprintln!("  3. Install the binary:");
    eprintln!(
        "     cargo install cairo-native --version {required_version} --bin {binary_name} --locked"
    );
    eprintln!();
    eprintln!("📋 Or use the installation script:");
    eprintln!("   ./scripts/install_compilers.sh");
    eprintln!();
    eprintln!("💡 For more information about cairo-native requirements:");
    eprintln!("   https://github.com/lambdaclass/cairo_native/blob/main/README.md");
    eprintln!();
    eprintln!(
        "ℹ️  In CI, ensure LLVM dependencies are installed and binaries are available in PATH."
    );
    eprintln!("   The binary will be used as a subprocess for Sierra → Native compilation.\n");
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
