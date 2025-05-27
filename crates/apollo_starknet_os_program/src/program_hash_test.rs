use std::path::PathBuf;
use std::sync::LazyLock;

use apollo_infra_utils::compile_time_cargo_manifest_dir;

use crate::program_hash::{
    compute_aggregator_program_hash,
    compute_os_program_hash,
    AggregatorHash,
    ProgramHashes,
};
use crate::PROGRAM_HASHES;

static PROGRAM_HASH_PATH: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(compile_time_cargo_manifest_dir!()).join("src/program_hash.json")
});

/// Asserts the program hash of the compiled Starknet OS program matches the program hash in the
/// JSON.
/// To fix this test, run the following command:
/// ```bash
/// FIX_PROGRAM_HASH=1 cargo test -p apollo_starknet_os_program test_program_hashes
/// ```
#[test]
fn test_program_hashes() {
    let AggregatorHash { with_prefix, without_prefix } = compute_aggregator_program_hash().unwrap();
    let computed_hashes = ProgramHashes {
        os: compute_os_program_hash().unwrap(),
        aggregator: without_prefix,
        aggregator_with_prefix: with_prefix,
    };
    if std::env::var("FIX_PROGRAM_HASH").is_ok() {
        std::fs::write(
            PROGRAM_HASH_PATH.as_path(),
            serde_json::to_string_pretty(&computed_hashes).unwrap(),
        )
        .unwrap_or_else(|error| panic!("Failed to write the program hash file: {error:?}."));
    } else {
        assert_eq!(computed_hashes, *PROGRAM_HASHES);
    }
}
