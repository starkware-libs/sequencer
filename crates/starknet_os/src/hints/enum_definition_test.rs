use std::collections::HashSet;
use std::sync::LazyLock;

use apollo_starknet_os_program::{AGGREGATOR_PROGRAM, OS_PROGRAM};
use blockifier::execution::hint_code::SYSCALL_HINTS;
use cairo_vm::hint_processor::builtin_hint_processor::hint_code::HINT_CODES;
use cairo_vm::hint_processor::builtin_hint_processor::kzg_da::WRITE_DIVMOD_SEGMENT;
use cairo_vm::hint_processor::builtin_hint_processor::secp::cairo0_hints::CAIRO0_HINT_CODES;
use cairo_vm::types::program::Program;
use strum::IntoEnumIterator;

use crate::hints::enum_definition::{AllHints, DeprecatedSyscallHint};
use crate::hints::types::HintEnum;

static VM_HINTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut vm_hints = HashSet::from([WRITE_DIVMOD_SEGMENT]);
    vm_hints.extend(HINT_CODES.values());
    vm_hints.extend(CAIRO0_HINT_CODES.values());
    vm_hints
});

fn program_hints(program: &Program) -> HashSet<String> {
    program
        .shared_program_data
        .hints_collection
        .iter_hints()
        .map(|hint| hint.code.clone())
        .collect()
}

fn unknown_hints_for_program(program: &Program) -> HashSet<String> {
    program_hints(program)
        .into_iter()
        .filter(|hint| AllHints::from_str(hint).is_err() && !VM_HINTS.contains(hint.as_str()))
        .collect()
}

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

#[test]
fn test_all_hints_are_known() {
    let unknown_os_hints = unknown_hints_for_program(&OS_PROGRAM);
    let unknown_aggregator_hints = unknown_hints_for_program(&AGGREGATOR_PROGRAM);
    let unknown_hints: HashSet<String> =
        unknown_os_hints.union(&unknown_aggregator_hints).cloned().collect();

    assert!(
        unknown_hints.is_empty(),
        "The following hints are not known in 'starknet_os': {unknown_hints:#?}."
    );
}

#[test]
fn test_all_hints_are_used() {
    let os_hints = program_hints(&OS_PROGRAM);
    let aggregator_hints = program_hints(&AGGREGATOR_PROGRAM);
    let all_program_hints: HashSet<&String> = os_hints.union(&aggregator_hints).collect();
    let redundant_hints: HashSet<_> = AllHints::all_iter()
        .filter(|hint| {
            // Skip syscalls; they do not appear in the OS code.
            !matches!(hint, AllHints::DeprecatedSyscallHint(_))
                && !all_program_hints.contains(&String::from(hint.to_str()))
        })
        .collect();
    assert!(
        redundant_hints.is_empty(),
        "The following hints are not used in the OS or Aggregator programs: {redundant_hints:#?}. \
         Please remove them from the enum definition."
    );
}
