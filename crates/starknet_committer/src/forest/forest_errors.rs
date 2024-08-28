use starknet_patricia::patricia_merkle_tree::filled_tree::errors::FilledTreeError;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::errors::UpdatedSkeletonTreeError;
use thiserror::Error;
use tokio::task::JoinError;

use crate::block_committer::input::ContractAddress;

pub(crate) type ForestResult<T> = Result<T, ForestError>;

#[derive(Debug, Error)]
pub enum ForestError {
    #[error(transparent)]
    OriginalSkeleton(#[from] OriginalSkeletonTreeError),
    #[error(transparent)]
    UpdatedSkeleton(#[from] UpdatedSkeletonTreeError),
    #[error("Couldn't create Classes Trie: {0}")]
    ClassesTrie(#[source] FilledTreeError),
    #[error("Couldn't create Contracts Trie: {0}")]
    ContractsTrie(#[source] FilledTreeError),
    #[error("Missing input: Couldn't find the storage trie's current state of address {0:?}")]
    MissingContractCurrentState(ContractAddress),
    #[error(
        "Can't build storage trie's updated skeleton, because there is no original skeleton at \
         address {0:?}"
    )]
    MissingOriginalSkeleton(ContractAddress),
    #[error(
        "Can't create Contracts trie, because there is no updated skeleton for storage trie at \
         address {0:?}"
    )]
    MissingUpdatedSkeleton(ContractAddress),
    #[error(
        "Can't build storage trie, because there are no sorted leaf indices of the contract at \
         address {0:?}"
    )]
    MissingSortedLeafIndices(ContractAddress),
    #[error(transparent)]
    JoinError(#[from] JoinError),
    #[error("Couldn't create Storage Trie: {0}")]
    StorageTrie(#[source] FilledTreeError),
}
