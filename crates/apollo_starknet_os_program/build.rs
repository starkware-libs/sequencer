use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::{compile_cairo0_program, Cairo0CompilerError};
use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Build script for the `apollo_starknet_os_program` crate.
/// Recompiles the OS program if the source files change.
fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set."));

    println!("cargo::warning=Compiling Starknet OS program...");
    let starknet_os_bytes = compile_starknet_os();
    println!("cargo::warning=Done. Writing compiled bytes to output directory.");
    let starknet_os_bytes_path = out_dir.join("starknet_os_bytes");
    std::fs::write(&starknet_os_bytes_path, &starknet_os_bytes)
        .expect("Failed to write the compiled OS bytes to the output directory.");

    println!("cargo::warning=Compiling Starknet aggregator program...");
    let starknet_aggregator_bytes = compile_starknet_aggregator();
    println!("cargo::warning=Done. Writing compiled bytes to output directory.");
    let starknet_aggregator_bytes_path = out_dir.join("starknet_aggregator_bytes");
    std::fs::write(&starknet_aggregator_bytes_path, &starknet_aggregator_bytes)
        .expect("Failed to write the compiled aggregator bytes to the output directory.");
}

fn compile_program(path_to_main_file: PathBuf) -> Vec<u8> {
    match compile_cairo0_program(path_to_main_file, cairo_root_path()) {
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
            panic!("Failed to compile the program. Error:\n{other_error}.")
        }
    }
}

fn cairo_root_path() -> PathBuf {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo")
}

fn compile_starknet_os() -> Vec<u8> {
    compile_program(cairo_root_path().join("starkware/starknet/core/os/os.cairo"))
}

fn compile_starknet_aggregator() -> Vec<u8> {
    compile_program(cairo_root_path().join("starkware/starknet/core/aggregator/main.cairo"))
}
