use rstest::rstest;

use super::*;

#[rstest]
#[case::ascii("a plain revert reason".to_string())]
#[case::multibyte("שלום".repeat(10))]
fn short_contract_call_error_is_relayed_verbatim(#[case] error_text: String) {
    assert!(error_text.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    assert_eq!(truncate_contract_call_error(error_text.clone()), error_text);
}

/// The cap counts bytes, so a multi-byte reason must be cut at a character boundary at or just
/// below it, never mid-character.
#[rstest]
#[case::single_byte_characters("a")]
#[case::four_byte_characters("😀")]
fn long_contract_call_error_is_truncated_on_a_character_boundary(#[case] repeated_text: &str) {
    const NUM_REPETITIONS: usize = 1000;
    let error_text = repeated_text.repeat(NUM_REPETITIONS);
    let truncated = truncate_contract_call_error(error_text.clone());

    let head = truncated
        .strip_suffix(TRUNCATION_MARKER)
        .expect("Truncated text must carry the truncation marker");
    assert!(error_text.starts_with(head), "the kept head must be a prefix of the original");
    assert!(head.len() <= MAX_CONTRACT_CALL_ERROR_BYTES);
    // Nothing is dropped beyond what the boundary requires.
    assert!(head.len() > MAX_CONTRACT_CALL_ERROR_BYTES - repeated_text.len());
}
