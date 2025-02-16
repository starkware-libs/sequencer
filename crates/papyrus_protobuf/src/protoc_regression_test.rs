use std::{env, fs};

use tempfile::tempdir;

use crate::regression_test_utils::{PROTO_DIR, PROTO_FILES, generate_protos};

#[test]
fn test_proto_regression() {
    let out_dir = tempdir().unwrap();

    let fix = env::var("PROTO_FIX").is_ok();

    generate_protos(out_dir.path().to_path_buf(), PROTO_FILES).unwrap();

    let generated_name = String::from(out_dir.path().to_str().unwrap()) + "/_.rs";
    let expected_name = String::from(PROTO_DIR) + "/protoc_output.rs";

    let expected_file = fs::read_to_string(expected_name.clone())
        .unwrap_or_else(|_| panic!("Failed to read expected file at {:?}", expected_name));
    let generated_file = fs::read_to_string(generated_name.clone())
        .unwrap_or_else(|_| panic!("Failed to read generated file at {:?}", generated_name));

    if expected_file != generated_file {
        if fix {
            fs::copy(generated_name, expected_name).expect("Failed to fix the precompiled protos");
        } else {
            panic!(
                "Generated protos are different from precompiled protos. Run `PROTO_FIX=1 cargo \
                 test -p papyrus_protobuf test_proto_regression` to fix the precompiled protos."
            );
        }
    }
}
