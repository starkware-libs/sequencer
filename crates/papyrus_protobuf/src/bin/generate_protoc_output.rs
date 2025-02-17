use papyrus_protobuf::regression_test_utils::{generate_protos, PROTO_DIR, PROTO_FILES};

fn main() {
    generate_protos(PROTO_DIR.into(), PROTO_FILES).unwrap();
}
