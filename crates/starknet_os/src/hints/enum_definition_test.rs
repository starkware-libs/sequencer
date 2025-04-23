use std::collections::HashSet;

use blockifier::execution::hint_code::SYSCALL_HINTS;
use strum::IntoEnumIterator;

use crate::hints::enum_definition::{AllHints, DeprecatedSyscallHint};
use crate::hints::types::HintEnum;

#[test]
fn test_hint_strings_are_unique() {
    let all_hints = AllHints::all_iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let all_hints_set: HashSet<&&str> = HashSet::from_iter(all_hints.iter());
    assert_eq!(all_hints.len(), all_hints_set.len(), "Duplicate hint strings.");
}

#[test]
fn test_from_str_for_all_hints() {
    for hint in AllHints::all_iter() {
        let hint_str = hint.to_str();
        let parsed_hint = AllHints::from_str(hint_str).unwrap();
        assert_eq!(hint, parsed_hint, "Failed to parse hint: {hint_str}.");
    }
}

#[test]
fn test_syscall_compatibility_with_blockifier() {
    let syscall_hint_strings =
        DeprecatedSyscallHint::iter().map(|hint| hint.to_str()).collect::<HashSet<_>>();
    let blockifier_syscall_strings: HashSet<_> = SYSCALL_HINTS.iter().cloned().collect();
    assert_eq!(
        blockifier_syscall_strings, syscall_hint_strings,
        "The syscall hints in the 'blockifier' do not match the syscall hints in 'starknet_os'.
        If this is intentional, please update the 'starknet_os' hints and add a todo to update 
        the implementation."
    );
}
