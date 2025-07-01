use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::regression_test_utils::{generate_protos, PROTOC_OUTPUT, PROTO_DIR, PROTO_FILES};

#[test]
fn protoc_output_matches_result_of_running_protoc() {
    let out_dir = tempdir().unwrap();

    generate_protos(out_dir.path().to_path_buf(), PROTO_FILES).unwrap();

    let generated_name = PathBuf::from(out_dir.path()).join("_.rs");
    let expected_name = PathBuf::from(PROTO_DIR).join(PROTOC_OUTPUT);

    let expected_file = fs::read_to_string(&expected_name)
        .unwrap_or_else(|_| panic!("Failed to read expected file at {expected_name:?}"));
    let generated_file = fs::read_to_string(&generated_name)
        .unwrap_or_else(|_| panic!("Failed to read generated file at {generated_name:?}"));

    // Using assert instead of assert_eq to avoid showing the entire content of the files on
    // assertion fail
    assert!(
        expected_file == generated_file,
        "Generated protos are different from precompiled protos. Run 'cargo run --bin \
         generate_protoc_output -q --features bin-deps' to override precompiled protos with newly \
         generated."
    );
}
