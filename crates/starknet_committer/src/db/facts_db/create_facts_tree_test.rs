use rstest::rstest;
use rstest_reuse::apply;
use starknet_patricia::patricia_merkle_tree::external_test_utils::MockLeaf;

use crate::db::create_original_skeleton_tests::{
    create_tree_cases,
    test_create_original_skeleton,
    CreateTreeCase,
};
use crate::db::facts_db::FactsNodeLayout;

#[apply(create_tree_cases)]
#[rstest]
#[tokio::test]
async fn test_create_tree_facts_layout(
    #[case] mut case: CreateTreeCase,
    #[values(true, false)] compare_modified_leaves: bool,
) {
    test_create_original_skeleton::<MockLeaf, FactsNodeLayout>(
        &mut case.storage,
        &case.leaf_modifications,
        case.root_hash,
        &case.expected_skeleton_nodes,
        case.subtree_height,
        compare_modified_leaves,
    )
    .await;
}
