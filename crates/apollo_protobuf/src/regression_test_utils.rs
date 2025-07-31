use std::path::PathBuf;
use std::process::Command;
use std::{env, io};

use tracing::{debug, info};

pub const PROTO_DIR: &str = "src/protobuf";
pub const PROTO_FILES: &[&str] = &[
    "src/proto/p2p/proto/class.proto",
    "src/proto/p2p/proto/consensus/consensus.proto",
    "src/proto/p2p/proto/mempool/transaction.proto",
    "src/proto/p2p/proto/sync/class.proto",
    "src/proto/p2p/proto/sync/event.proto",
    "src/proto/p2p/proto/sync/header.proto",
    "src/proto/p2p/proto/sync/state.proto",
    "src/proto/p2p/proto/sync/receipt.proto",
    "src/proto/p2p/proto/sync/transaction.proto",
    "src/proto/p2p/proto/transaction.proto",
];
pub const PROTOC_OUTPUT: &str = "protoc_output.rs";

pub fn generate_protos(out_dir: PathBuf, proto_files: &[&str]) -> Result<(), io::Error> {
    info!("Building protos");
    debug!("Files: {:?}", proto_files);

    // OUT_DIR env variable is required by protoc_prebuilt
    env::set_var("OUT_DIR", &out_dir);

    if get_valid_preinstalled_protoc_version().is_none() {
        info!(
            "Protoc is not installed. Adding a prebuilt protoc binary via gh actions before \
             building."
        );
        let (protoc_bin, _) = protoc_prebuilt::init("27.0").expect(
        "Build failed due to Github's rate limit. Please run `gh auth login` to lift the rate limit and allow protoc compilation to proceed. \
        If this issue persists please download Protoc following the instructions at http://protobuf.dev/installation/",
        );
        info!("Prebuilt protoc added to the project.");
        env::set_var("PROTOC", protoc_bin);
    }

    // Using absolute paths for consistency between test and bin
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    prost_build::Config::new()
        .protoc_arg(format!("--proto_path={}", project_root.display()))
        .out_dir(&out_dir)
        .compile_protos(
            &proto_files
                .iter()
                .map(|p| project_root.join(p))
                .collect::<Vec<_>>()
                .iter()
                .map(|p| p.to_str().unwrap())
                .collect::<Vec<_>>(),
            &[project_root.join("src/proto").to_str().unwrap()],
        )?;

    Ok(())
}

/// Returns the version of the preinstalled protoc if it is valid (version 3.15.x or greater).
/// Otherwise, returns None.
fn get_valid_preinstalled_protoc_version() -> Option<(u32, u32)> {
    let protoc = env::var("PROTOC").unwrap_or("protoc".to_string());

    let protoc_version_output =
        String::from_utf8_lossy(&Command::new(protoc).arg("--version").output().ok()?.stdout)
            .to_string();

    let parts: Vec<&str> = protoc_version_output.split_whitespace().collect();
    // The returned string is in the format "libprotoc 25.1". We need to extract the version
    let protoc_version_str = parts.get(1)?;
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
