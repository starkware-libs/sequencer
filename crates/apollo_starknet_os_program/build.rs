use std::path::PathBuf;
use std::process::Command;

use apollo_infra_utils::cairo0_compiler::{cairo0_compilers_correct_version, CAIRO0_COMPILE};
use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set.");
    println!("cargo::warning=Compiling Starknet OS program...");
    let starknet_os_bytes = compile_starknet_os();
    println!("cargo::warning=Done. Writing compiled bytes to output directory.");
    let starknet_os_bytes_path = PathBuf::from(out_dir).join("starknet_os_bytes");
    std::fs::write(&starknet_os_bytes_path, &starknet_os_bytes)
        .expect("Failed to write the compiled OS bytes to the output directory.");
}

/// Compile the StarkNet OS program.
fn compile_starknet_os() -> Vec<u8> {
    cairo0_compilers_correct_version().unwrap_or_else(|error| {
        panic!(
            "Failed to verify correct cairo0 package installation. Please make sure you do not \
             have a conflicting installation in your {}/bin directory.\nOriginal error: {error:?}.",
            std::env::var("CARGO_HOME").unwrap_or("${CARGO_HOME}".to_string())
        )
    });
    let cairo_root_path = PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");
    let os_main_path = cairo_root_path.join("starkware/starknet/core/os/os.cairo");
    assert!(os_main_path.exists(), "OS main file does not exist at {os_main_path:?}.");
    let mut compile_os_command = Command::new(CAIRO0_COMPILE);
    compile_os_command.args([
        os_main_path.to_str().expect("Path is valid unicode."),
        "--debug_info_with_source",
        "--cairo_path",
        cairo_root_path.to_str().expect("Path to cairo is valid unicode."),
    ]);
    println!("cargo::warning=Running command {compile_os_command:?}.");
    let compile_os_output =
        compile_os_command.output().expect("Failed to run the OS compile command.");

    // Verify output.
    if !compile_os_output.status.success() {
        let stderr = String::from_utf8_lossy(&compile_os_output.stderr);
        panic!("Failed to compile the OS. Error: {}", stderr.trim());
    }

    compile_os_output.stdout
}
