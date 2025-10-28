use std::fs;
use std::path::Path;

use rstest::rstest;
use tempfile::TempDir;

use crate::utils::copy_dir_contents;

/// Helper function to create a test directory structure in a temporary directory.
/// Returns the TempDir to keep it alive for the duration of the test.
fn create_test_dir_structure() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create files
    fs::write(base.join("file1.txt"), "content1").unwrap();
    fs::write(base.join("file2.txt"), "content2").unwrap();

    // Create subdirectory with files
    fs::create_dir(base.join("subdir")).unwrap();
    fs::write(base.join("subdir/file3.txt"), "content3").unwrap();
    fs::write(base.join("subdir/file4.txt"), "content4").unwrap();

    // Create nested subdirectory
    fs::create_dir(base.join("subdir/nested")).unwrap();
    fs::write(base.join("subdir/nested/file5.txt"), "content5").unwrap();

    temp_dir
}

/// Verifies the test directory structure created by create_test_dir_structure.
fn verify_test_dir_structure(dir: &Path) {
    assert!(dir.exists());
    assert!(dir.is_dir());

    // Verify top-level files
    assert!(dir.join("file1.txt").exists());
    assert!(dir.join("file2.txt").exists());
    assert_eq!(fs::read_to_string(dir.join("file1.txt")).unwrap(), "content1");
    assert_eq!(fs::read_to_string(dir.join("file2.txt")).unwrap(), "content2");

    // Verify subdir and its files
    assert!(dir.join("subdir").is_dir());
    assert!(dir.join("subdir/file3.txt").exists());
    assert!(dir.join("subdir/file4.txt").exists());
    assert_eq!(fs::read_to_string(dir.join("subdir/file3.txt")).unwrap(), "content3");
    assert_eq!(fs::read_to_string(dir.join("subdir/file4.txt")).unwrap(), "content4");

    // Verify nested subdir and its file
    assert!(dir.join("subdir/nested").is_dir());
    assert!(dir.join("subdir/nested/file5.txt").exists());
    assert_eq!(fs::read_to_string(dir.join("subdir/nested/file5.txt")).unwrap(), "content5");
}

#[rstest]
fn test_copy_basic_structure() {
    let temp_src = create_test_dir_structure();
    let temp_dst = TempDir::new().unwrap();

    copy_dir_contents(temp_src.path(), temp_dst.path());

    verify_test_dir_structure(temp_dst.path());
}

#[rstest]
fn test_copy_to_nonexistent_destination() {
    let temp_src = create_test_dir_structure();
    let temp_parent = TempDir::new().unwrap();
    let dst = temp_parent.path().join("new_dir");

    assert!(!dst.exists());
    copy_dir_contents(temp_src.path(), &dst);

    verify_test_dir_structure(&dst);
}

#[rstest]
fn test_copy_overwrites_existing_files() {
    let temp_src = TempDir::new().unwrap();
    fs::write(temp_src.path().join("file.txt"), "new content").unwrap();

    let temp_dst = TempDir::new().unwrap();
    fs::write(temp_dst.path().join("file.txt"), "old content").unwrap();

    copy_dir_contents(temp_src.path(), temp_dst.path());

    assert_eq!(fs::read_to_string(temp_dst.path().join("file.txt")).unwrap(), "new content");
}

#[rstest]
/// Tests that copying a directory to the same path does nothing.
fn test_copy_same_path() {
    let temp = create_test_dir_structure();

    copy_dir_contents(temp.path(), temp.path());

    verify_test_dir_structure(temp.path());
}

#[rstest]
#[should_panic(expected = "Source path is not a directory: /nonexistent")]
fn test_copy_nonexistent_source_panics() {
    let temp_dst = TempDir::new().unwrap();
    copy_dir_contents(Path::new("/nonexistent"), temp_dst.path());
}
