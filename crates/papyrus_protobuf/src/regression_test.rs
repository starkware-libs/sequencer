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
    // if OUT_DIR is not set, we need to create a temp dir and set the env var
    let out_dir = match env::var("OUT_DIR") {
        Ok(dir) => &dir.clone(),
        Err(_) => {
            std::fs::create_dir_all(OUT_DIR).expect("Failed to create temp OUT_DIR");
            env::set_var("OUT_DIR", OUT_DIR);
            OUT_DIR
        }
    };
    // in test mode we need to set the OUT_DIR env var
    let fix = env::var("PROTO_FIX").is_ok();

    // remove the temp dir if it exists (can happen if the test failed previously)
    if Path::new(out_dir).exists() {
        fs::remove_dir_all(out_dir).unwrap();
    }
    fs::create_dir(out_dir).unwrap();

    generate_protos(out_dir.into(), PROTO_FILES).unwrap();

    // let expected = fs::read_dir(PROTO_DIR)
    //     .expect("Failed to read precompiled proto dir")
    //     .next()
    //     .expect("No files in precompiled proto dir")
    //     .expect("Failed to read precompiled protos")
    //     .path();
    // let generated = fs::read_dir(out_dir)
    //     .expect("Failed to read generated proto dir")
    //     .next()
    //     .expect("No files in generated proto dir")
    //     .expect("Failed to read generated protos")
    //     .path();

    let generated_name = String::from(out_dir) + "/_.rs"; // "src/generated_test/_.rs";
    let expected_name = String::from(PROTO_DIR) + "/_.rs"; // "src/protoc_output/_.rs";

    let expected_file = fs::read_to_string(expected_name.clone())
        .unwrap_or_else(|_| panic!("Failed to read expected file at {:?}", expected_name));
    let generated_file = fs::read_to_string(generated_name.clone())
        .unwrap_or_else(|_| panic!("Failed to read generated file at {:?}", generated_name));
    let equal = expected_file == generated_file;

    if !equal {
        if fix {
            fs::copy(generated_name, expected_name).expect("Failed to fix the precompiled protos");
        } else {
            panic!("Generated protos are different from precompiled protos");
        }
    }

    // remove the temp dir
    fs::remove_dir_all(out_dir).unwrap();
}
