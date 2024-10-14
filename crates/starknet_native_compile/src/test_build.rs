use crate::build_utils::binary_path;
use std::process::Command;

#[test]
fn test_build() {
    let binary_path = binary_path();
    assert!(binary_path.exists());
    println!(
        "Binary version: {}",
        String::from_utf8(Command::new(&binary_path).args(["--version"]).output().unwrap().stdout)
            .unwrap()
    );
}
