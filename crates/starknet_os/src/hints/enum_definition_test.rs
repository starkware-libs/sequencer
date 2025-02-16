use std::collections::HashSet;

use blockifier::execution::hint_code::SYSCALL_HINTS;
use strum::IntoEnumIterator;

use super::{HintExtension, OsHint, SyscallHint};
use crate::hints::types::HintEnum;

#[test]
fn test_hint_strings_are_unique() {
    let hint_strings = OsHint::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_extension_strings =
        HintExtension::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let syscall_hint_strings = SyscallHint::iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let hint_strings_set: HashSet<&&str> = HashSet::from_iter(hint_strings.iter());
    let hint_extension_strings_set = HashSet::from_iter(hint_extension_strings.iter());
    let syscall_hint_strings_set: HashSet<&&str> = HashSet::from_iter(syscall_hint_strings.iter());
    assert_eq!(hint_strings.len(), hint_strings_set.len(), "Duplicate hint strings.");
    assert_eq!(
        hint_extension_strings.len(),
        hint_extension_strings_set.len(),
        "Duplicate hint extension strings."
    );
    assert_eq!(
        syscall_hint_strings.len(),
        syscall_hint_strings_set.len(),
        "Duplicate syscall hint strings."
    );

    assert!(
        hint_strings_set.is_disjoint(&hint_extension_strings_set),
        "{:?}",
        hint_strings_set.intersection(&hint_extension_strings_set).collect::<Vec<_>>()
    );
    assert!(
        hint_strings_set.is_disjoint(&syscall_hint_strings_set),
        "{:?}",
        hint_strings_set.intersection(&syscall_hint_strings_set).collect::<Vec<_>>()
    );
    assert!(
        hint_extension_strings_set.is_disjoint(&syscall_hint_strings_set),
        "{:?}",
        hint_extension_strings_set.intersection(&syscall_hint_strings_set).collect::<Vec<_>>()
    );
}

#[test]
fn test_syscall_compatibility_with_blockifier() {
    let syscall_hint_strings =
        SyscallHint::iter().map(|hint| hint.to_str()).collect::<HashSet<_>>();
    let blockifier_syscall_strings: HashSet<_> = SYSCALL_HINTS.iter().cloned().collect();
    assert_eq!(
        blockifier_syscall_strings, syscall_hint_strings,
        "The syscall hints in the 'blockifier' do not match the syscall hints in 'starknet_os'.
        If this is intentional, please update the 'starknet_os' hints and add a todo to update 
        the implementation."
    );
}
