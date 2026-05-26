use rstest::rstest;
use rstest_reuse::apply;
use starknet_patricia::patricia_merkle_tree::external_test_utils::MockLeaf;
use starknet_patricia_storage::db_object::EmptyKeyContext;

use crate::db::create_original_skeleton_tests::{
    create_tree_cases,
    test_create_original_skeleton,
    CreateTreeCase,
};
use crate::db::index_db::test_utils::convert_facts_db_to_index_db;
use crate::db::index_db::IndexNodeLayout;
use crate::hash_function::mock_hash::MockTreeHashFunction;

#[apply(create_tree_cases)]
#[rstest]
#[tokio::test]
async fn test_create_tree_index_layout(
    #[case] mut case: CreateTreeCase,
    #[values(true, false)] compare_modified_leaves: bool,
) {
    let mut storage = convert_facts_db_to_index_db::<MockLeaf, MockLeaf, EmptyKeyContext>(
        &mut case.storage,
        case.root_hash,
        &EmptyKeyContext,
        &mut None,
    )
    .await;

    test_create_original_skeleton::<MockLeaf, IndexNodeLayout<MockTreeHashFunction>>(
        &mut storage,
        &case.leaf_modifications,
        case.root_hash,
        &case.expected_skeleton_nodes,
        case.subtree_height,
        compare_modified_leaves,
    )
    .await;
}
