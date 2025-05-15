use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::{compile_cairo0_program, Cairo0CompilerError};
use apollo_infra_utils::compile_time_cargo_manifest_dir;

/// Compile the StarkNet OS program.
pub fn compile_starknet_os() -> Vec<u8> {
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
