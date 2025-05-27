use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;

use crate::program_hash::{compute_os_program_hash, ProgramHash};
use crate::PROGRAM_HASH;

static PROGRAM_HASH_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/program_hash.json")
});

/// Asserts the program hash of the compiled Starknet OS program matches the program hash in the
/// JSON.
/// To fix this test, run the following command:
/// ```bash
/// FIX_PROGRAM_HASH=1 cargo test -p apollo_starknet_os_program test_program_hash
/// ```
#[test]
fn test_program_hash() {
    let computed_hash = ProgramHash { os: compute_os_program_hash().unwrap() };
    if std::env::var("FIX_PROGRAM_HASH").is_ok() {
        std::fs::write(
            PROGRAM_HASH_PATH.as_path(),
            serde_json::to_string_pretty(&computed_hash).unwrap(),
        )
        .unwrap_or_else(|error| panic!("Failed to write the program hash file: {error:?}."));
    } else {
        assert_eq!(computed_hash, *PROGRAM_HASH);
    }
}
