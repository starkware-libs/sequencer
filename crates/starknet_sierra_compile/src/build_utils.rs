use std::env;
use std::path::{Path, PathBuf};

include!("constants.rs");

fn set_out_dir_env_var_for_runtime() {
    // Get the OUT_DIR environment variable.
    let out_dir = env::var("OUT_DIR").unwrap();

    // Tell Cargo to pass this variable to the compiler
    println!("cargo:rustc-env={}={}", RUNTIME_ACCESSIBLE_OUT_DIR_ENV_VAR_NAME, out_dir);
}

fn out_dir() -> PathBuf {
    Path::new(
        &std::env::var(RUNTIME_ACCESSIBLE_OUT_DIR_ENV_VAR_NAME)
            .expect("Failed to get the OUT_DIR environment variable"),
    )
    .to_path_buf()
}

/// Get the crate's `OUT_DIR` and navigate up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared across all crates in this project.
fn target_dir() -> PathBuf {
    let out_dir = out_dir();

    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

fn shared_folder_dir() -> PathBuf {
    target_dir().join("shared_executables")
}

pub fn binary_path(binary_name: &str) -> PathBuf {
    shared_folder_dir().join(binary_name)
}

#[cfg(feature = "cairo_native")]
pub fn output_file_path() -> String {
    out_dir().join("output.so").to_str().unwrap().into()
}

#[cfg(feature = "cairo_native")]
pub fn repo_root_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").to_path_buf()
}
