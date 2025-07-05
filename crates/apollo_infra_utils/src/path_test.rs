use crate::path::{path_of_project_root, resolve_project_relative_path};

#[test]
fn resolve_project_relative_path_on_non_existent_path() {
    let relative_path = "does_not_exist.txt";
    let expected_path = path_of_project_root().join(relative_path);
    assert!(!expected_path.exists());
    let result = resolve_project_relative_path(relative_path);

    assert!(result.is_err(), "Expected an non-existent path error, got {result:?}");
}

#[test]
fn resolve_project_relative_path_success() {
    let relative_path = std::file!();
    let result = resolve_project_relative_path(relative_path);

    assert!(result.unwrap().ends_with(relative_path));
}
