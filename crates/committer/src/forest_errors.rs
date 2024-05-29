use crate::block_committer::input::ContractAddress;
use crate::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use crate::patricia_merkle_tree::node_data::leaf::{LeafData, LeafDataImpl};
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;

use thiserror::Error;
use tokio::task::JoinError;

pub(crate) type ForestResult<T> = Result<T, ForestError<LeafDataImpl>>;

#[derive(Debug, Error)]
pub(crate) enum ForestError<L: LeafData> {
    #[error(transparent)]
    OriginalSkeleton(#[from] OriginalSkeletonTreeError),
    #[error(transparent)]
    UpdatedSkeleton(#[from] UpdatedSkeletonTreeError),
    #[error(transparent)]
    Filled(#[from] FilledTreeError<L>),
    #[error("Missing input: Couldn't find the storage trie's current state of address {0:?}")]
    MissingContractCurrentState(ContractAddress),
    #[error("Can't build storage trie's updated skeleton, because there is no original skeleton at address {0:?}")]
    MissingOriginalSkeleton(ContractAddress),
    #[error("Can't fill storage trie, because there is no updated skeleton at address {0:?}")]
    MissingUpdatedSkeleton(ContractAddress),
    #[error(transparent)]
    JoinError(#[from] JoinError),
}
