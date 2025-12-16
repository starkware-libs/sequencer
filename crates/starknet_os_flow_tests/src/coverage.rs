use std::collections::HashSet;
use std::fs;

use expect_test::{expect, expect_file, Expect};
use itertools::Itertools;
use rstest::rstest;
use starknet_os::hints::enum_definition::AllHints;

pub(crate) fn expect_hint_coverage(unused_hints: &HashSet<AllHints>, test_name: &str) {
    let covered_hints = AllHints::all_iter()
        .filter(|hint| !unused_hints.contains(hint))
        .sorted()
        .collect::<Vec<_>>();
    expect_file![format!("../resources/hint_coverage/{test_name}.json")]
        .assert_eq(&serde_json::to_string_pretty(&covered_hints).unwrap());
}

const UNCOVERED_HINTS: Expect = expect![[r#"
    [
        "AggregatorHint(DisableDaPageCreation)",
        "AggregatorHint(GetAggregatorOutput)",
        "AggregatorHint(GetChainIdFromInput)",
        "AggregatorHint(GetFeeTokenAddressFromInput)",
        "AggregatorHint(GetFullOutputFromInput)",
        "AggregatorHint(GetOsOuputForInnerBlocks)",
        "AggregatorHint(GetPublicKeysFromAggregatorInput)",
        "AggregatorHint(GetUseKzgDaFromInput)",
        "AggregatorHint(WriteDaSegment)",
        "DeprecatedSyscallHint(DelegateCall)",
        "DeprecatedSyscallHint(DelegateL1Handler)",
        "DeprecatedSyscallHint(Deploy)",
        "OsHint(GetClassHashAndCompiledClassFact)",
        "OsHint(InitializeAliasCounter)",
        "OsHint(LoadBottom)",
        "StatelessHint(SetApToSegmentHashPoseidon)",
    ]
"#]];

/// Tests that the set of uncovered hints is up to date.
#[rstest]
fn test_coverage_regression() {
    // Iterate over all JSON files in the coverage directory.
    let covered_hints = fs::read_dir("resources/hint_coverage")
        .unwrap()
        .map(|entry| entry.unwrap())
        .flat_map(|entry| {
            serde_json::from_str::<Vec<AllHints>>(&fs::read_to_string(entry.path()).unwrap())
                .unwrap()
        })
        .unique()
        .collect::<Vec<_>>();
    let uncovered_hints = AllHints::all_iter()
        .filter(|hint| !covered_hints.contains(hint))
        .map(|hint| format!("{hint:?}"))
        .sorted()
        .collect::<Vec<_>>();
    UNCOVERED_HINTS.assert_debug_eq(&uncovered_hints);
}
