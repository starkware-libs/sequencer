use std::env;
use std::io::{Error, ErrorKind, Result};
use std::process::Command;

fn parse_protoc_version(protoc_version_str: &str) -> Result<Vec<u32>> {
    Ok(protoc_version_str.split('.').map(|part| part.parse::<u32>().unwrap()).collect())
}

fn validate_preinstalled_protoc() -> Result<()> {
    let protoc = env::var("PROTOC").unwrap_or("protoc".to_string());

    let protoc_version =
        String::from_utf8_lossy(&Command::new(protoc).arg("--version").output()?.stdout)
            .to_string();

    let parts: Vec<&str> = protoc_version.split_whitespace().collect();
    // let protoc_version_str = parts.get(1).expect("Failed to determine protoc version");
    let protoc_version_str = match parts.get(1) {
        Some(version) => version,
        None => return Err(Error::new(ErrorKind::Other, "protoc version not found")),
    };
    let mut protoc_version_parts = parse_protoc_version(protoc_version_str)?.into_iter();
    let major = protoc_version_parts.next().expect("Protoc version did not have a major number");
    let minor = protoc_version_parts.next().unwrap_or(0);

    if major < 3 || (major == 3 && minor < 15) {
        Err(Error::new(
            ErrorKind::Other,
            "protoc version is too old. version 3.15.x or greater is needed.",
        ))
    } else {
        Ok(())
    }
}

fn main() -> Result<()> {
    // If Protoc is installed use it, if not complie using prebuilt protoc.
    println!("Building");
    if validate_preinstalled_protoc().is_err() {
        println!("Building using prebuilt protoc");
        let (protoc_bin, _) = protoc_prebuilt::init("27.0").expect(
        "Please run `gh auth login` to enable protoc compilation.\n
        If this issue persists please download Protoc following the instructions at https://github.com/starkware-libs/sequencer/blob/main/docs/papyrus/README.adoc#prerequisites",
        );
        env::set_var("PROTOC", protoc_bin);
    }
    prost_build::compile_protos(
        &[
            "src/proto/p2p/proto/rpc_transaction.proto",
            "src/proto/p2p/proto/class.proto",
            "src/proto/p2p/proto/event.proto",
            "src/proto/p2p/proto/header.proto",
            "src/proto/p2p/proto/state.proto",
            "src/proto/p2p/proto/transaction.proto",
            "src/proto/p2p/proto/consensus.proto",
        ],
        &["src/proto/"],
    )?;
    Ok(())
}
