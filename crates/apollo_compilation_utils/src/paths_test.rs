use std::fs;

use tempfile::TempDir;

use crate::paths::exe_relative_tools_root;

#[test]
fn tools_directory_next_to_executable_is_resolved() {
    let package_dir = TempDir::new().unwrap();
    let tools_dir = package_dir.path().join("tools");
    fs::create_dir(&tools_dir).unwrap();
    let exe_path = package_dir.path().join("prover_binary");

    assert_eq!(exe_relative_tools_root(&exe_path), Some(tools_dir));
}

#[test]
fn missing_tools_directory_yields_none() {
    let package_dir = TempDir::new().unwrap();
    let exe_path = package_dir.path().join("prover_binary");

    assert_eq!(exe_relative_tools_root(&exe_path), None);
}

#[test]
fn tools_path_that_is_a_file_yields_none() {
    let package_dir = TempDir::new().unwrap();
    fs::write(package_dir.path().join("tools"), "not a directory").unwrap();
    let exe_path = package_dir.path().join("prover_binary");

    assert_eq!(exe_relative_tools_root(&exe_path), None);
}
