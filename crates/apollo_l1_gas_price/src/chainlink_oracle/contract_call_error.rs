//! Truncation of the batcher error text a failed view call relays.

#[cfg(test)]
#[path = "contract_call_error_test.rs"]
mod contract_call_error_test;

/// Cap on the batcher error text the Chainlink oracle relays. A reverting view call's panic data
/// reaches the logs, the failure cache, and (when the provider runs remotely) the RPC boundary, so
/// the cap is byte-based to bound what all three consume.
pub(super) const MAX_CONTRACT_CALL_ERROR_BYTES: usize = 256;
pub(super) const TRUNCATION_MARKER: &str = "...[truncated]";

pub(super) fn truncate_contract_call_error(error_text: String) -> String {
    if error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES {
        return error_text;
    }
    // Cut on a character boundary so the relayed text stays valid UTF-8. The nearest boundary at
    // or below the cap is at most three bytes down.
    let head_end = (0..=MAX_CONTRACT_CALL_ERROR_BYTES)
        .rev()
        .find(|byte_index| error_text.is_char_boundary(*byte_index))
        .expect("Byte index 0 is always a character boundary");
    format!("{}{TRUNCATION_MARKER}", &error_text[..head_end])
}
