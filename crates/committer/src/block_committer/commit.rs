use crate::block_committer::errors::BlockCommitmentError;
use crate::block_committer::input::{Input, StateDiff};
use crate::patricia_merkle_tree::original_skeleton_tree::skeleton_forest::{
    OriginalSkeletonForest, OriginalSkeletonForestImpl,
};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::storage::map_storage::MapStorage;

#[allow(dead_code)]
type BlockCommitmentResult<T> = Result<T, BlockCommitmentError>;
#[allow(dead_code)]
pub(crate) fn commit_block(input: Input) -> BlockCommitmentResult<()> {
    let original_forest =
        OriginalSkeletonForestImpl::<OriginalSkeletonTreeImpl>::create_original_skeleton_forest(
            MapStorage::from(input.storage),
            input.contracts_trie_root_hash,
            input.classes_trie_root_hash,
            input.tree_heights,
            &input.current_contracts_trie_leaves,
            &input.state_diff,
        )?;
    let _updated_forest = original_forest
        .compute_updated_skeleton_forest::<UpdatedSkeletonTreeImpl>(
            &StateDiff::actual_classes_updates(
                &input.state_diff.class_hash_to_compiled_class_hash,
                input.tree_heights,
            ),
            &input.state_diff.accessed_addresses(),
            &input.state_diff.actual_storage_updates(input.tree_heights),
        )?;

    todo!()
}
