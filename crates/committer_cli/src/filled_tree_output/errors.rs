use std::fmt::Debug;

use starknet_committer::patricia_merkle_tree::types::{
    ClassesTrieError,
    ContractsTrieError,
    StorageTrieError,
};

#[derive(thiserror::Error, Debug)]
pub enum FilledForestError {
    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
    #[error(transparent)]
    StorageTrie(#[from] StorageTrieError),
    #[error(transparent)]
    ClassesTrie(#[from] ClassesTrieError),
    #[error(transparent)]
    ContractsTrie(#[from] ContractsTrieError),
}
