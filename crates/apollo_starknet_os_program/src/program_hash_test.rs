use crate::program_hash::{
    compute_aggregator_program_hash,
    compute_os_program_hash,
    AggregatorHash,
    ProgramHash,
    PROGRAM_HASH_PATH,
};
use crate::PROGRAM_HASH;

/// Asserts the program hash of the compiled Starknet OS program matches the program hash in the
/// JSON.
/// To fix this test, run the following command:
/// ```bash
/// FIX_PROGRAM_HASH=1 cargo test -p apollo_starknet_os_program test_program_hash
/// ```
#[test]
fn test_program_hash() {
    let AggregatorHash { with_prefix, without_prefix } = compute_aggregator_program_hash().unwrap();
    let computed_hash = ProgramHash {
        os: compute_os_program_hash().unwrap(),
        aggregator: without_prefix,
        aggregator_with_prefix: with_prefix,
    };
    if std::env::var("FIX_PROGRAM_HASH").is_ok() {
        std::fs::write(
            PROGRAM_HASH_PATH.as_path(),
            serde_json::to_string_pretty(&computed_hash).unwrap(),
        )
        .expect("Failed to write the program hash file.");
    } else {
        assert_eq!(computed_hash, *PROGRAM_HASH);
    }
}
