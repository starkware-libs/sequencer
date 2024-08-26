use std::path::{Path, PathBuf};

fn out_dir() -> PathBuf {
    Path::new(&std::env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable"))
        .to_path_buf()
}

/// Get the crate's `OUT_DIR` and navigates up to reach the `target/BUILD_FLAVOR` directory.
/// This directory is shared accross all crates in this project.
fn target_dir() -> PathBuf {
    let out_dir = out_dir();

    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

pub fn cairo_folder_dir() -> PathBuf {
    target_dir().join("cairo")
}
