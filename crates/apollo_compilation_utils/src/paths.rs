// Note: This module includes path resolution functions that are needed during build and run times.
// It must not contain functionality that is available in only in one of these modes. Specifically,
// it must avoid relying on env variables such as 'CARGO_*' or 'OUT_DIR'.

fn target_dir(out_dir: &std::path::Path) -> std::path::PathBuf {
    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf()
}

pub fn shared_folder_dir(out_dir: &std::path::Path) -> std::path::PathBuf {
    target_dir(out_dir).join("shared_executables")
}

pub fn binary_path(out_dir: &std::path::Path, binary_name: &str) -> std::path::PathBuf {
    shared_folder_dir(out_dir).join(binary_name)
}
