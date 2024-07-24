use std::fmt::Debug;

use committer::patricia_merkle_tree::filled_tree::errors::{
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
