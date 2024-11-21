use assert_matches::assert_matches;

use crate::path::{path_of_project_root, resolve_project_relative_path, PathResolutionError};

// TODO: Add a test for PathResolutionError::IoError.
#[test]
fn resolve_project_relative_path_on_non_existent_path() {
    let relative_path = "does_not_exist.txt";
    let expected_path = path_of_project_root().join(relative_path);
    assert!(!expected_path.exists());
    let result = resolve_project_relative_path(relative_path);
    assert_matches!(
        result, Err(PathResolutionError::PathDoesNotExist { path }) if path == expected_path
    );
}

#[test]
fn resolve_project_relative_path_success() {
    let relative_path = std::file!();
    let result = resolve_project_relative_path(relative_path);

    assert_matches!(result, Ok(path) if path.ends_with(relative_path));
}
