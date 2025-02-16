use papyrus_protobuf::regression_test_utils::{PROTO_DIR, PROTO_FILES, generate_protos};

fn main() {
    generate_protos(PROTO_DIR.into(), PROTO_FILES).unwrap();
}
