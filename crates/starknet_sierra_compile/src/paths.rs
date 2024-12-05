// Note: This module includes constants that are needed during build and run times. It must
// not contain functionality that is available in only in one of these modes. Specifically, it
// must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

fn out_dir() -> std::path::PathBuf {
    std::path::Path::new(
        &std::env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable"),
    )
    .to_path_buf()
}

/// Get the crate's `OUT_DIR` and navigate up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared across all crates in this project.
fn target_dir() -> std::path::PathBuf {
    let out_dir = out_dir();

    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

fn shared_folder_dir() -> std::path::PathBuf {
    target_dir().join("shared_executables")
}

pub fn binary_path(binary_name: &str) -> std::path::PathBuf {
    shared_folder_dir().join(binary_name)
}

#[cfg(feature = "cairo_native")]
pub fn output_file_path() -> String {
    out_dir().join("output.so").to_str().unwrap().into()
}

#[cfg(feature = "cairo_native")]
pub fn repo_root_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").to_path_buf()
}
