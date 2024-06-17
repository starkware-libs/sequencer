use committer::patricia_merkle_tree::filled_tree::errors::{
    ClassesTrieError, ContractsTrieError, StorageTrieError,
};
use std::fmt::Debug;

#[derive(thiserror::Error, Debug)]
pub(crate) enum FilledForestError {
    #[error(transparent)]
    SerializationError(#[from] serde_json::Error),
    #[error(transparent)]
    StorageTrie(#[from] StorageTrieError),
    #[error(transparent)]
    ClassesTrie(#[from] ClassesTrieError),
    #[error(transparent)]
    ContractsTrie(#[from] ContractsTrieError),
}
