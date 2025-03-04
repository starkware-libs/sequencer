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
