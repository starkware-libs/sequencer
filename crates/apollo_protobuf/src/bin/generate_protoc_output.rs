use std::fs;
use std::path::Path;

use apollo_protobuf::regression_test_utils::{
    generate_protos,
    PROTOC_OUTPUT,
    PROTO_DIR,
    PROTO_FILES,
};

fn main() {
    let out_dir = String::from("crates/apollo_protobuf/") + PROTO_DIR;

    generate_protos(out_dir.clone().into(), PROTO_FILES).unwrap();

    // TODO(alonl): Consider using tonic_build to allow naming the file directly instead of renaming
    // here
    fs::rename(Path::new(&out_dir).join("_.rs"), Path::new(&out_dir).join(PROTOC_OUTPUT)).unwrap();

    for file in fs::read_dir(out_dir).unwrap() {
        let file = file.unwrap();
        let file_name = file.file_name().into_string().unwrap();
        if file_name != PROTOC_OUTPUT {
            if file.path().is_file() {
                fs::remove_file(file.path()).unwrap();
            } else {
                fs::remove_dir_all(file.path()).unwrap();
            }
        }
    }
}
