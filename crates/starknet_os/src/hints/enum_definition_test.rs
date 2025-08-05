use std::collections::HashSet;
use std::sync::LazyLock;

use apollo_starknet_os_program::{AGGREGATOR_PROGRAM, OS_PROGRAM};
use blockifier::execution::deprecated_syscalls::DeprecatedSyscallSelector;
use blockifier::execution::execution_utils::sn_api_to_cairo_vm_program;
use blockifier::execution::hint_code::SYSCALL_HINTS;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::hint_processor::builtin_hint_processor::hint_code::HINT_CODES;
use cairo_vm::hint_processor::builtin_hint_processor::kzg_da::WRITE_DIVMOD_SEGMENT;
use cairo_vm::hint_processor::builtin_hint_processor::secp::cairo0_hints::CAIRO0_HINT_CODES;
use cairo_vm::types::program::Program;
use rstest::{fixture, rstest};
use starknet_api::deprecated_contract_class::ContractClass;
use strum::IntoEnumIterator;

use crate::hints::enum_definition::{
    AggregatorHint,
    AllHints,
    CommonHint,
    DeprecatedSyscallHint,
    HintExtension,
    OsHint,
    StatelessHint,
    TEST_HINT_PREFIX,
};
use crate::hints::types::HintEnum;

static VM_HINTS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut vm_hints = HashSet::from([WRITE_DIVMOD_SEGMENT]);
    vm_hints.extend(HINT_CODES.values());
    vm_hints.extend(CAIRO0_HINT_CODES.values());
    vm_hints
});

// Whitelist for Blake2s hints that are not yet used in the Cairo programs.
static BLAKE2_HINTS_WHITELIST: LazyLock<HashSet<AllHints>> = LazyLock::new(|| {
    HashSet::from([
        // TODO(Aviv): Remove these hints once we have used them in the OS.
        AllHints::StatelessHint(StatelessHint::CheckPackedValuesEndAndSize),
        AllHints::StatelessHint(StatelessHint::UnpackFeltsToU32s),
        AllHints::StatelessHint(StatelessHint::SetApToSegmentHashBlake),
    ])
});

// Whitelist for Blake2s hint strings that are not yet used in the Cairo programs.
static BLAKE2_HINT_STRINGS_WHITELIST: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| BLAKE2_HINTS_WHITELIST.iter().map(|hint| hint.to_str()).collect());

/// This conversion is only required for testing consistency.
impl TryFrom<DeprecatedSyscallSelector> for DeprecatedSyscallHint {
    type Error = String;

    fn try_from(selector: DeprecatedSyscallSelector) -> Result<Self, Self::Error> {
        match selector {
            DeprecatedSyscallSelector::CallContract => Ok(Self::CallContract),
            DeprecatedSyscallSelector::DelegateCall => Ok(Self::DelegateCall),
            DeprecatedSyscallSelector::DelegateL1Handler => Ok(Self::DelegateL1Handler),
            DeprecatedSyscallSelector::Deploy => Ok(Self::Deploy),
            DeprecatedSyscallSelector::EmitEvent => Ok(Self::EmitEvent),
            DeprecatedSyscallSelector::GetBlockNumber => Ok(Self::GetBlockNumber),
            DeprecatedSyscallSelector::GetBlockTimestamp => Ok(Self::GetBlockTimestamp),
            DeprecatedSyscallSelector::GetCallerAddress => Ok(Self::GetCallerAddress),
            DeprecatedSyscallSelector::GetContractAddress => Ok(Self::GetContractAddress),
            DeprecatedSyscallSelector::GetSequencerAddress => Ok(Self::GetSequencerAddress),
            DeprecatedSyscallSelector::GetTxInfo => Ok(Self::GetTxInfo),
            DeprecatedSyscallSelector::GetTxSignature => Ok(Self::GetTxSignature),
            DeprecatedSyscallSelector::LibraryCall => Ok(Self::LibraryCall),
            DeprecatedSyscallSelector::LibraryCallL1Handler => Ok(Self::LibraryCallL1Handler),
            DeprecatedSyscallSelector::ReplaceClass => Ok(Self::ReplaceClass),
            DeprecatedSyscallSelector::SendMessageToL1 => Ok(Self::SendMessageToL1),
            DeprecatedSyscallSelector::StorageRead => Ok(Self::StorageRead),
            DeprecatedSyscallSelector::StorageWrite => Ok(Self::StorageWrite),
            _ => Err(format!("Non-deprecated syscall selector: {selector:?}.")),
        }
    }
}

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

#[fixture]
fn vm_hints() -> HashSet<String> {
    VM_HINTS.iter().map(|s| s.to_string()).collect()
}

#[fixture]
fn common_hints() -> HashSet<String> {
    CommonHint::iter().map(|hint| hint.to_str().to_string()).collect()
}

#[fixture]
fn stateless_hints() -> HashSet<String> {
    StatelessHint::iter().map(|hint| hint.to_str().to_string()).collect()
}

#[fixture]
fn os_hints() -> HashSet<String> {
    OsHint::iter().map(|hint| hint.to_str().to_string()).collect()
}

#[fixture]
fn aggregator_hints() -> HashSet<String> {
    AggregatorHint::iter().map(|hint| hint.to_str().to_string()).collect()
}

#[fixture]
fn hint_extension() -> HashSet<String> {
    HintExtension::iter().map(|hint| hint.to_str().to_string()).collect()
}

#[fixture]
fn os_program_hints() -> HashSet<String> {
    program_hints(&OS_PROGRAM)
}

#[fixture]
fn aggregator_program_hints() -> HashSet<String> {
    program_hints(&AGGREGATOR_PROGRAM)
}

#[fixture]
fn vm_union_stateless(
    vm_hints: HashSet<String>,
    stateless_hints: HashSet<String>,
) -> HashSet<String> {
    vm_hints.union(&stateless_hints).cloned().collect()
}

#[rstest]
fn test_hint_strings_are_unique() {
    let all_hints = AllHints::all_iter().map(|hint| hint.to_str()).collect::<Vec<_>>();
    let all_hints_set: HashSet<&&str> = HashSet::from_iter(all_hints.iter());
    assert_eq!(all_hints.len(), all_hints_set.len(), "Duplicate hint strings.");
}

#[rstest]
fn test_from_str_for_all_hints() {
    for hint in AllHints::all_iter() {
        let hint_str = hint.to_str();
        let parsed_hint = AllHints::from_str(hint_str).unwrap();
        assert_eq!(hint, parsed_hint, "Failed to parse hint: {hint_str}.");
    }
}

#[rstest]
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

#[rstest]
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

/// Tests that we do not keep any hint including the TEST_HINT_PREFIX as a prefix in the OS or
/// aggregator code.
#[rstest]
fn test_the_debug_hint_isnt_merged(
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
) {
    let all_program_hints: HashSet<&String> =
        os_program_hints.union(&aggregator_program_hints).collect();

    let debug_hints: HashSet<_> =
        all_program_hints.iter().filter(|hint| hint.trim().starts_with(TEST_HINT_PREFIX)).collect();

    assert!(
        debug_hints.is_empty(),
        "The following debug hints are present in the OS or Aggregator programs: \
         {debug_hints:#?}. Please remove them from the code."
    );
}

#[rstest]
fn test_all_hints_are_used(
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
) {
    let all_program_hints: HashSet<&String> =
        os_program_hints.union(&aggregator_program_hints).collect();
    let redundant_hints: HashSet<_> = AllHints::all_iter()
        .filter(|hint| {
            // Skip syscalls and whitelisted hints that do not appear in the OS code.
            !matches!(hint, AllHints::DeprecatedSyscallHint(_))
                && !BLAKE2_HINTS_WHITELIST.contains(hint)
                && !all_program_hints.contains(&String::from(hint.to_str()))
        })
        .collect();
    assert!(
        redundant_hints.is_empty(),
        "The following hints are not used in the OS or Aggregator programs: {redundant_hints:#?}. \
         Please remove them from the enum definition."
    );
}

/// Tests that the set of deprecated syscall hints is consistent with the enum of deprecated
/// syscalls.
#[rstest]
fn test_deprecated_syscall_hint_consistency() {
    let deprecated_syscall_hints: Vec<DeprecatedSyscallHint> =
        DeprecatedSyscallHint::iter().collect();
    let deprecated_syscall_selectors: Vec<DeprecatedSyscallSelector> =
        DeprecatedSyscallSelector::iter()
            .filter(|selector| {
                !matches!(
                    selector,
                    // As the new and deprecated syscall selector enums are the same enum,
                    // explicitly filter out all "new" syscalls that are not supported in Cairo0.
                    DeprecatedSyscallSelector::GetBlockHash
                        | DeprecatedSyscallSelector::GetClassHashAt
                        | DeprecatedSyscallSelector::GetExecutionInfo
                        | DeprecatedSyscallSelector::Keccak
                        | DeprecatedSyscallSelector::KeccakRound
                        | DeprecatedSyscallSelector::Sha256ProcessBlock
                        | DeprecatedSyscallSelector::MetaTxV0
                        | DeprecatedSyscallSelector::Secp256k1Add
                        | DeprecatedSyscallSelector::Secp256k1GetPointFromX
                        | DeprecatedSyscallSelector::Secp256k1GetXy
                        | DeprecatedSyscallSelector::Secp256k1Mul
                        | DeprecatedSyscallSelector::Secp256k1New
                        | DeprecatedSyscallSelector::Secp256r1Add
                        | DeprecatedSyscallSelector::Secp256r1GetPointFromX
                        | DeprecatedSyscallSelector::Secp256r1GetXy
                        | DeprecatedSyscallSelector::Secp256r1Mul
                        | DeprecatedSyscallSelector::Secp256r1New
                )
            })
            .collect();

    assert_eq!(
        deprecated_syscall_selectors.len(),
        deprecated_syscall_hints.len(),
        "The number of deprecated syscall selectors does not match the number of deprecated \
         syscall hints. Selectors: {deprecated_syscall_selectors:#?}, hints: \
         {deprecated_syscall_hints:#?}",
    );

    let converted_selectors: HashSet<DeprecatedSyscallHint> = deprecated_syscall_selectors
        .iter()
        .map(|selector| DeprecatedSyscallHint::try_from(*selector).unwrap())
        .collect();
    assert_eq!(
        converted_selectors,
        deprecated_syscall_hints.iter().cloned().collect(),
        "The deprecated syscall selectors, converted to hints, do not match the deprecated \
         syscall hints. Converted selectors: {converted_selectors:#?}, hints: \
         {deprecated_syscall_hints:#?}"
    );
}

#[rstest]
/// If OP = OS program hints, AP = aggregator program hints, VM = VM hints,
/// S = `StatelessHint`, C = `CommonHint`, then we verify that:
/// C = (OP ∩ AP) \ VM \ S
fn test_common_hints_in_both_os_and_aggregator_programs(
    common_hints: HashSet<String>,
    vm_union_stateless: HashSet<String>,
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
) {
    let common_program_hints: HashSet<String> = os_program_hints
        .intersection(&aggregator_program_hints)
        .filter(|hint| !vm_union_stateless.contains(hint.as_str()))
        .cloned()
        .collect();

    if common_program_hints != common_hints {
        let missing_in_common_hints: HashSet<_> =
            common_program_hints.difference(&common_hints).cloned().collect();
        let extra_in_common_hints: HashSet<_> =
            common_hints.difference(&common_program_hints).cloned().collect();
        panic!(
            "The Common hints should contain exactly the common program hints, excluding VM and \
             stateless hints. Missing in Common hints: {missing_in_common_hints:#?}, Extra in \
             Common hints: {extra_in_common_hints:#?}"
        );
    }
}

#[rstest]
/// If OP = OS program hints, AP = aggregator program hints, VM = VM hints,
/// S = `StatelessHint`, then we verify that:
/// S ⊆ (OP ∪ AP) \ VM
fn test_stateless_hints_in_os_or_aggregator_programs(
    stateless_hints: HashSet<String>,
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
    vm_hints: HashSet<String>,
) {
    let all_program_hints_excluding_vm: HashSet<String> = os_program_hints
        .union(&aggregator_program_hints)
        .filter(|hint| !vm_hints.contains(hint.as_str()))
        .cloned()
        .collect();
    let difference = stateless_hints
        .difference(&all_program_hints_excluding_vm)
        .filter(|hint| !BLAKE2_HINT_STRINGS_WHITELIST.contains(hint.as_str()))
        .cloned()
        .collect::<HashSet<_>>();

    assert!(
        difference.is_empty(),
        "The following stateless hints are not present in the OS or Aggregator programs: \
         {difference:#?}."
    );
}

#[rstest]
/// If A = `AggregatorHint` enum hints, OP = OS program hints, AP = aggregator program hints, VM =
/// VM hints, S = `StatelessHint`, then we verify that:
/// A = AP \ (VM ∪ OP ∪ S)
fn test_aggregator_hints_are_unique_aggregator_program_hints(
    aggregator_hints: HashSet<String>,
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
    vm_union_stateless: HashSet<String>,
) {
    let union_os_program_vm_stateless: HashSet<String> =
        vm_union_stateless.union(&os_program_hints).cloned().collect();
    let unique_aggregator_program_hints: HashSet<String> =
        aggregator_program_hints.difference(&union_os_program_vm_stateless).cloned().collect();

    if unique_aggregator_program_hints != aggregator_hints {
        let missing_in_aggregator_hints: HashSet<_> =
            unique_aggregator_program_hints.difference(&aggregator_hints).cloned().collect();
        let extra_in_aggregator_hints: HashSet<_> =
            aggregator_hints.difference(&unique_aggregator_program_hints).cloned().collect();
        panic!(
            "The Aggregator hints should contain exactly the unique Aggregator program hints. \
             Missing in Aggregator hints: {missing_in_aggregator_hints:#?}, Extra in Aggregator \
             hints: {extra_in_aggregator_hints:#?}"
        );
    }
}

#[rstest]
/// If O = `OsHint` enum hints, OP = OS program hints, A = `AggregatorHint` enum hints, VM = VM
/// hints, S = `StatelessHint`, E = `ExtensionHint`, then we verify that:
/// O ∪ E = OP \ (AP ∪ VM ∪ S)
fn test_os_hints_are_unique_os_program_hints(
    os_hints: HashSet<String>,
    hint_extension: HashSet<String>,
    os_program_hints: HashSet<String>,
    aggregator_program_hints: HashSet<String>,
    vm_union_stateless: HashSet<String>,
) {
    let union_aggregator_program_vm_stateless: HashSet<String> =
        vm_union_stateless.union(&aggregator_program_hints).cloned().collect();
    let os_union_extension: HashSet<String> = os_hints.union(&hint_extension).cloned().collect();
    let unique_os_program_hints: HashSet<String> =
        os_program_hints.difference(&union_aggregator_program_vm_stateless).cloned().collect();

    if unique_os_program_hints != os_union_extension {
        let missing_in_os_or_extension: HashSet<_> =
            unique_os_program_hints.difference(&os_union_extension).cloned().collect();
        let extra_in_os_or_extension: HashSet<_> =
            os_union_extension.difference(&unique_os_program_hints).cloned().collect();
        panic!(
            "The OS & Extension hints should contain exactly the unique OS program hints. Missing \
             in OS or Extension hints: {missing_in_os_or_extension:#?}, Extra in OS or Extension \
             hints: {extra_in_os_or_extension:#?}"
        );
    }
}

/// Tests that the deprecated syscall hint strings match the strings in compiled Cairo0 contracts.
/// If a new deprecated syscall was added, it should be added to the `other_syscalls` function of
/// the Cairo0 test contract.
#[rstest]
fn test_deprecated_syscall_hint_strings() {
    let test_contract: ContractClass =
        serde_json::from_str(&FeatureContract::TestContract(CairoVersion::Cairo0).get_raw_class())
            .unwrap();
    let test_program = sn_api_to_cairo_vm_program(test_contract.program).unwrap();
    let test_program_hints = program_hints(&test_program);
    for hint in DeprecatedSyscallHint::iter() {
        if matches!(
            hint,
            DeprecatedSyscallHint::DelegateCall | DeprecatedSyscallHint::DelegateL1Handler
        ) {
            // The delegate syscalls have been removed from cairo-lang (the hint string cannot
            // change), so (a) they cannot be tested by recompiling the test contract, and (b) they
            // should not be regression-tested: a flow test that invokes these syscalls is enough.
            continue;
        }
        let hint_str = hint.to_str();
        assert!(
            test_program_hints.contains(hint_str),
            "The deprecated syscall hint '{hint_str}' is not present in the test contract hints."
        );
    }
}
