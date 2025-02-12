use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs, io};

const OUT_DIR: &str = "src/generated_test"; // temp (?) dir to output the generated files
// TODO(alonl): figure out how to enforce this path to match the one in protobuf.rs
const PROTO_DIR: &str = "src/protoc_output"; // dir containing the current proto files
const PROTO_FILES: &[&str] = &[
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

pub fn generate_protos(out_dir: PathBuf, proto_files: &[&str]) -> Result<(), io::Error> {
    println!("Building protos");
    if get_valid_preinstalled_protoc_version().is_none() {
        println!(
            "Protoc is not installed. Adding a prebuilt protoc binary via gh actions before \
             building."
        );
        let (protoc_bin, _) = protoc_prebuilt::init("27.0").expect(
        "Build failed due to Github's rate limit. Please run `gh auth login` to lift the rate limit and allow protoc compilation to proceed. \
        If this issue persists please download Protoc following the instructions at https://github.com/starkware-libs/sequencer/blob/main/docs/papyrus/README.adoc#prerequisites",
        );
        println!("Prebuilt protoc added to the project.");
        env::set_var("PROTOC", protoc_bin);
    }
    let _ = std::fs::create_dir_all(&out_dir);

    prost_build::Config::new().out_dir(out_dir).compile_protos(proto_files, &["src/proto/"])?;

    Ok(())
}

#[test]
fn test_proto_regression() {
    // in test mode we need to set the OUT_DIR env var
    if env::var("OUT_DIR").is_err() {
        std::fs::create_dir_all(OUT_DIR).expect("Failed to create temp OUT_DIR");
        env::set_var("OUT_DIR", OUT_DIR);
    }
    let fix = env::var("PROTO_FIX").is_ok();

    // remove the temp dir if it exists (can happen if the test failed previously)
    if Path::new(OUT_DIR).exists() {
        fs::remove_dir_all(OUT_DIR).unwrap();
    }
    fs::create_dir(OUT_DIR).unwrap();

    generate_protos(OUT_DIR.into(), PROTO_FILES).unwrap();

    let expected = fs::read_dir(PROTO_DIR)
        .expect("Failed to read precompiled proto dir")
        .next()
        .expect("No files in precompiled proto dir")
        .expect("Failed to read precompiled protos")
        .path();
    let generated = fs::read_dir(OUT_DIR)
        .expect("Failed to read generated proto dir")
        .next()
        .expect("No files in generated proto dir")
        .expect("Failed to read generated protos")
        .path();

    let expected_file = fs::read_to_string(expected).expect("Failed to read expected file");
    let generated_file = fs::read_to_string(generated).expect("Failed to read generated file");
    assert_eq!(expected_file, generated_file);

    if fix {
        fs::copy(generated_file, expected_file).expect("Failed to fix the precompiled protos");
    }

    // remove the temp dir
    fs::remove_dir_all(OUT_DIR).unwrap();
}
