use std::env::{set_var, var};

use protoc_prebuilt::init;
use tonic_build::{configure, Builder};

fn main() {
    if var("PROFILE").unwrap() == "debug" {
        set_var("PROTOC_PREBUILT_NOT_ADD_GITHUB_TOKEN", "true");
        let (protoc_bin, _) = init("27.0").unwrap();
        set_var("PROTOC", protoc_bin);

        let builder: Builder = configure();
        builder
            .compile(
                &[
                    "tests/common/proto/component_a_service.proto",
                    "tests/common/proto/component_b_service.proto",
                ],
                &["tests/common/proto"],
            )
            .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
    }
}
