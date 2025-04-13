use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::{compile_cairo0_program, Cairo0CompilerError};
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
    let cairo_root_path = PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo");
    let os_main_path = cairo_root_path.join("starkware/starknet/core/os/os.cairo");
    match compile_cairo0_program(os_main_path, cairo_root_path) {
        Ok(bytes) => bytes,
        Err(Cairo0CompilerError::Cairo0CompilerVersion(error)) => {
            panic!(
                "Failed to verify correct cairo0 package installation. Please make sure you do \
                 not have a conflicting installation in your {}/bin directory.\nOriginal error: \
                 {error:?}.",
                std::env::var("CARGO_HOME").unwrap_or("${CARGO_HOME}".to_string())
            )
        }
        Err(other_error) => {
            panic!("Failed to compile the StarkNet OS program. Error:\n{other_error}.")
        }
    }
}
