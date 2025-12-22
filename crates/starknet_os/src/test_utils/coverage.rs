use std::collections::HashSet;

use expect_test::expect_file;
use itertools::Itertools;

use crate::hints::enum_definition::AllHints;

pub fn expect_hint_coverage(unused_hints: &HashSet<AllHints>, test_name: &str) {
    let covered_hints = AllHints::all_iter()
        .filter(|hint| !unused_hints.contains(hint))
        .sorted()
        .collect::<Vec<_>>();
    expect_file![format!("../../resources/hint_coverage/{test_name}.json")]
        .assert_eq(&serde_json::to_string_pretty(&covered_hints).unwrap());
}

#[cfg(test)]
const UNCOVERED_HINTS: expect_test::Expect = expect_test::expect![[r#"
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
        "DeprecatedSyscallHint(Deploy)",
        "OsHint(GetClassHashAndCompiledClassFact)",
        "OsHint(InitializeAliasCounter)",
        "StatelessHint(SetApToSegmentHashPoseidon)",
    ]
"#]];

/// Tests that the set of uncovered hints is up to date.
#[cfg(test)]
#[rstest::rstest]
fn test_coverage_regression() {
    // Iterate over all JSON files in the coverage directory.
    let covered_hints = std::fs::read_dir("resources/hint_coverage")
        .unwrap()
        .map(|entry| entry.unwrap())
        .flat_map(|entry| {
            serde_json::from_str::<Vec<AllHints>>(&std::fs::read_to_string(entry.path()).unwrap())
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
