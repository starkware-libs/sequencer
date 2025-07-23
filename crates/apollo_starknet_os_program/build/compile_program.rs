use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::{compile_cairo0_program, Cairo0CompilerError};
use apollo_infra_utils::compile_time_cargo_manifest_dir;

pub async fn compile_and_output_program(
    out_dir: PathBuf,
    path_to_main_file_from_cairo_root: &str,
    program_name: &str,
) {
    println!("cargo::warning=Compiling {program_name} program...");
    let bytes = match compile_cairo0_program(
        cairo_root_path().join(path_to_main_file_from_cairo_root),
        cairo_root_path(),
    ) {
        Ok(bytes) => bytes,
        Err(Cairo0CompilerError::Cairo0CompilerVersion(error)) => {
            panic!(
                "Failed to verify correct cairo0 package installation. Please make sure you do \
                 not have a conflicting installation in your {}/bin directory.\nOriginal error: \
                 {error}.",
                std::env::var("CARGO_HOME").unwrap_or("${CARGO_HOME}".to_string())
            )
        }
        Err(other_error) => {
            panic!("Failed to compile the {program_name} program. Error:\n{other_error}.")
        }
    };
    println!(
        "cargo::warning=Done compiling {program_name}. Writing compiled bytes to output directory."
    );
    let bytes_path = out_dir.join(format!("{program_name}_bytes"));
    std::fs::write(&bytes_path, &bytes).unwrap_or_else(|error| {
        panic!("Failed to write the compiled {program_name} bytes to {bytes_path:?}: {error}.")
    });
}

#[cfg(feature = "test_programs")]
pub async fn compile_test_contracts(out_dir: PathBuf) {
    let mut task_set = tokio::task::JoinSet::new();
    task_set.spawn(compile_and_output_program(
        out_dir.clone(),
        "starkware/starknet/core/os/state/aliases_test.cairo",
        "aliases_test",
    ));
    task_set.spawn(compile_and_output_program(
        out_dir,
        "starkware/starknet/core/os/contract_class/blake_compiled_class_hash.cairo",
        "blake_compiled_class_hash",
    ));
    task_set.join_all().await;
}

fn cairo_root_path() -> PathBuf {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/cairo")
}
