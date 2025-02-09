use std::collections::HashSet;

use blockifier::execution::hint_code::SYSCALL_HINTS;
use strum::IntoEnumIterator;

use super::{HintExtension, OsHint, Syscall};
use crate::hints::types::HintEnum;

#[test]
fn test_hint_strings_are_unique() {
    let hint_strings = OsHint::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_extension_strings =
        HintExtension::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_strings_set: HashSet<&&str> = HashSet::from_iter(hint_strings.iter());
    let hint_extension_strings_set = HashSet::from_iter(hint_extension_strings.iter());
    assert_eq!(hint_strings.len(), hint_strings_set.len(), "Duplicate hint strings.");
    assert_eq!(
        hint_extension_strings.len(),
        hint_extension_strings_set.len(),
        "Duplicate hint extension strings."
    );
    let ambiguous_strings =
        hint_strings_set.intersection(&hint_extension_strings_set).collect::<Vec<_>>();
    assert!(ambiguous_strings.is_empty(), "Ambiguous hint strings: {ambiguous_strings:?}");
}

#[test]
fn test_syscall_compatibility_with_blockifier() {
    let syscall_hint_strings = Syscall::iter().map(|hint| hint.to_str()).collect::<HashSet<_>>();
    let blockifier_syscall_strings: HashSet<_> = SYSCALL_HINTS.iter().cloned().collect();
    assert_eq!(blockifier_syscall_strings, syscall_hint_strings);
}
