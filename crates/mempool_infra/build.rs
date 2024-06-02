use std::env::set_var;

use protoc_prebuilt::init;
use tonic_build::{configure, Builder};

fn main() {
    let (protoc_bin, _) = init("27.0").unwrap();
    set_var("PROTOC", protoc_bin);

    let builder: Builder = configure();
    builder
        .compile(
            &["proto/component_a_service.proto", "proto/component_b_service.proto"],
            &["proto"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
}
