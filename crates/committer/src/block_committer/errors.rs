use thiserror::Error;

use crate::patricia_merkle_tree::{
    filled_tree::errors::FilledTreeError, node_data::leaf::LeafData,
    original_skeleton_tree::errors::OriginalSkeletonTreeError,
    updated_skeleton_tree::errors::UpdatedSkeletonTreeError,
};

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum BlockCommitmentError<L: LeafData> {
    #[error(transparent)]
    OriginalSkeleton(#[from] OriginalSkeletonTreeError),
    #[error(transparent)]
    UpdatedSkeleton(#[from] UpdatedSkeletonTreeError),
    #[error(transparent)]
    FilledTree(#[from] FilledTreeError<L>),
}
