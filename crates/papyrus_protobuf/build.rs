use std::env;
use std::io::Result;

use tonic_build::{configure, Builder};

fn main() -> Result<()> {
    println!("Building");
    let (protoc_bin, _) = protoc_prebuilt::init("27.0").unwrap();
    env::set_var("PROTOC", protoc_bin);
    let builder: Builder = configure();
    builder
        .compile(
            &[
                "src/proto/p2p/proto/class.proto",
                "src/proto/p2p/proto/event.proto",
                "src/proto/p2p/proto/header.proto",
                "src/proto/p2p/proto/state.proto",
                "src/proto/p2p/proto/transaction.proto",
                "src/proto/p2p/proto/consensus.proto",
            ],
            &["src/proto/"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
    Ok(())
}
