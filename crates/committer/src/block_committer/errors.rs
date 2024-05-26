use thiserror::Error;

use crate::patricia_merkle_tree::{
    original_skeleton_tree::errors::OriginalSkeletonTreeError,
    updated_skeleton_tree::errors::UpdatedSkeletonTreeError,
};

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum BlockCommitmentError {
    #[error(transparent)]
    BuildingOriginalSkeletonTree(#[from] OriginalSkeletonTreeError),
    #[error(transparent)]
    BuildingUpdatedSkeletonTree(#[from] UpdatedSkeletonTreeError),
}
