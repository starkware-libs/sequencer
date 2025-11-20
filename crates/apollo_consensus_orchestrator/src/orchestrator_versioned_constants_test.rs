use expect_test::expect_file;
use similar::{ChangeTag, TextDiff};
use starknet_api::block::StarknetVersion;
use strum::IntoEnumIterator;

use crate::orchestrator_versioned_constants::VersionedConstants;

/// Test that the differences between VCs of different versions do not regress.
#[test]
fn test_vc_diffs_regression() {
    let first_version = StarknetVersion::V0_14_0;
    let mut prev_vc = VersionedConstants::json_str(&first_version).unwrap();
    let mut prev_version = first_version;
    for version in StarknetVersion::iter().filter(|v| v > &first_version) {
        let current_vc = VersionedConstants::json_str(&version).unwrap();
        let diff = TextDiff::from_lines(prev_vc, current_vc);
        let diff_string = diff
            .iter_all_changes()
            .filter_map(|change| match change.tag() {
                ChangeTag::Equal => None,
                ChangeTag::Delete => {
                    Some(format!("-[{:3}]   {change}", change.old_index().unwrap()))
                }
                ChangeTag::Insert => {
                    Some(format!("+[{:3}]   {change}", change.new_index().unwrap()))
                }
            })
            .collect::<Vec<_>>()
            .join("");
        expect_file![format!(
            "../resources/versioned_constants_diff_regression/{prev_version}_{version}.txt"
        )]
        .assert_eq(&diff_string);
        prev_version = version;
        prev_vc = current_vc;
    }
}
