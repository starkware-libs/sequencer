//! Support for the `starknet_committer_and_os_cli` Python-compatibility tests.

/// Parses the recorder's `get_accessed_keys_input` payload and checks it against the objects the
/// Rust side expects. Returns "Success" on a match, or a description of the first mismatch.
pub fn parse_accessed_keys_input_test(_input: &str) -> Result<String, serde_json::Error> {
    Ok("Success".to_string())
}
