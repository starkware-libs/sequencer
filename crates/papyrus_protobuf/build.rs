use std::process::Command;
use std::{env, io};

/// Returns the version of the preinstalled protoc if it is valid (version 3.15.x or greater).
/// Otherwise, returns None.
fn get_valid_preinstalled_protoc_version() -> Option<(u32, u32)> {
    let protoc = env::var("PROTOC").unwrap_or("protoc".to_string());

    let protoc_version_output =
        String::from_utf8_lossy(&Command::new(protoc).arg("--version").output().ok()?.stdout)
            .to_string();

    let parts: Vec<&str> = protoc_version_output.split_whitespace().collect();
    // The returned string is in the format "libprotoc 25.1". We need to extract the version
    let protoc_version_str = match parts.get(1) {
        Some(version) => version,
        None => return None,
    };
    let (major, minor) = parse_protoc_version(protoc_version_str)?;
    // Protoc versions before 3.15 are not supported.
    if (major < 3) || (major == 3 && minor < 15) { None } else { Some((major, minor)) }
}

/// Return Result<(major, minor)> numbers. If the minor doesn't exist, return 0 as minor. If
/// the major doesn't exist, return None.
fn parse_protoc_version(protoc_version_str: &str) -> Option<(u32, u32)> {
    let mut version_numbers_str = protoc_version_str.split('.');
    let major: u32 = match version_numbers_str.next() {
        Some(major) => major.parse::<u32>().ok()?,
        None => return None,
    };
    let minor: u32 = match version_numbers_str.next() {
        Some(minor) => minor.parse::<u32>().ok()?,
        None => 0,
    };
    Some((major, minor))
}

fn main() -> io::Result<()> {
    // If Protoc is installed use it, if not compile using prebuilt protoc.
    println!("Building");
    if get_valid_preinstalled_protoc_version().is_none() {
        println!(
            "Protoc is not installed locally. Adding a prebuilt protoc binary via gh actions \
             before building."
        );
        let (protoc_bin, _) = protoc_prebuilt::init("27.0").expect(
        "Build failed due to Github's rate limit. Please run `gh auth login` to lift the rate limit and allow protoc compilation to proceed. \
        If this issue persists please download Protoc following the instructions at https://github.com/starkware-libs/sequencer/blob/main/docs/papyrus/README.adoc#prerequisites",
        );
        println!("Prebuilt protoc added to the project.");
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
