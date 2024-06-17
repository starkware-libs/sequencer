use crate::block_committer::errors::BlockCommitmentError;
use crate::block_committer::input::{Input, StateDiff};
use crate::patricia_merkle_tree::filled_tree::forest::FilledForestImpl;
use crate::patricia_merkle_tree::original_skeleton_tree::skeleton_forest::{
    OriginalSkeletonForest, OriginalSkeletonForestImpl,
};
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::skeleton_forest::{
    UpdatedSkeletonForest, UpdatedSkeletonForestImpl,
};
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::storage::map_storage::MapStorage;

#[allow(dead_code)]
type BlockCommitmentResult<T> = Result<T, BlockCommitmentError>;

#[allow(dead_code)]
pub async fn commit_block(input: Input) -> BlockCommitmentResult<FilledForestImpl> {
    let mut original_forest = OriginalSkeletonForestImpl::<OriginalSkeletonTreeImpl>::create(
        MapStorage::from(input.storage),
        input.contracts_trie_root_hash,
        input.classes_trie_root_hash,
        &input.current_contracts_trie_leaves,
        &input.state_diff,
    )?;

    let updated_forest = UpdatedSkeletonForestImpl::<UpdatedSkeletonTreeImpl>::create(
        &mut original_forest,
        &StateDiff::skeleton_classes_updates(&input.state_diff.class_hash_to_compiled_class_hash),
        &input.state_diff.skeleton_storage_updates(),
        &input.current_contracts_trie_leaves,
        &input.state_diff.address_to_class_hash,
        &input.state_diff.address_to_nonce,
    )?;

    Ok(
        FilledForestImpl::create::<UpdatedSkeletonTreeImpl, TreeHashFunctionImpl>(
            updated_forest,
            input.state_diff.actual_storage_updates(),
            StateDiff::actual_classes_updates(&input.state_diff.class_hash_to_compiled_class_hash),
            &input.current_contracts_trie_leaves,
            &input.state_diff.address_to_class_hash,
            &input.state_diff.address_to_nonce,
        )
        .await?,
    )
}
