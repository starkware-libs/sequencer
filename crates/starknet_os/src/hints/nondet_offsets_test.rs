use std::collections::HashSet;

use crate::hints::enum_definition::AllHints;
use crate::hints::nondet_offsets::NONDET_FP_OFFSETS;
use crate::hints::types::HintEnum;

#[test]
fn test_nondet_offset_strings() {
    for (hint, offset) in NONDET_FP_OFFSETS.iter() {
        let hint_str = hint.to_str();
        let expected_prefix = format!("memory[fp + {offset}]");
        assert!(
            hint_str.starts_with(&expected_prefix),
            "Mismatch between hint string and offset: expected '{expected_prefix}' as a prefix of \
             hint '{hint_str}'."
        );
    }
}

#[test]
fn test_nondet_hints_have_offsets() {
    let nondet_hints: HashSet<AllHints> =
        AllHints::all_iter().filter(|hint| hint.to_str().starts_with("memory[fp +")).collect();
    let hints_with_offsets: HashSet<AllHints> = NONDET_FP_OFFSETS.keys().copied().collect();
    assert_eq!(
        nondet_hints, hints_with_offsets,
        "Mismatch between hints with offsets and hints with 'memory[fp + ...]' prefix."
    );
}
