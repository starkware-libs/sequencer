use starknet_patricia_storage::errors::{DeserializationError, StorageError};
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, Storage};
use thiserror::Error;

use crate::patricia_merkle_tree::filled_tree::node::FilledNode;
use crate::patricia_merkle_tree::node_data::leaf::Leaf;
use crate::patricia_merkle_tree::types::SubTree;

#[derive(Debug, Error)]
pub enum TraversalError {
    #[error(
        "Unable to read from storage the storage key: {0:?} while building the original skeleton \
         tree."
    )]
    StorageRead(#[from] StorageError),
    #[error(
        "Failed to deserialize the storage value: {0:?} while building the original skeleton tree."
    )]
    Deserialization(#[from] DeserializationError),
}

pub type TraversalResult<T> = Result<T, TraversalError>;

// TODO(Aviv, 17/07/2024): Split between storage prefix implementation and function logic.
pub(crate) fn calculate_subtrees_roots<'a, L: Leaf>(
    subtrees: &[SubTree<'a>],
    storage: &impl Storage,
) -> TraversalResult<Vec<FilledNode<L>>> {
    let mut subtrees_roots = vec![];
    let db_keys: Vec<DbKey> = subtrees
        .iter()
        .map(|subtree| {
            create_db_key(subtree.get_root_prefix::<L>().into(), &subtree.root_hash.0.to_bytes_be())
        })
        .collect();

    let db_vals = storage.mget(&db_keys);
    for ((subtree, optional_val), db_key) in
        subtrees.iter().zip(db_vals.iter()).zip(db_keys.into_iter())
    {
        let val = optional_val.ok_or(StorageError::MissingKey(db_key))?;
        subtrees_roots.push(FilledNode::deserialize(subtree.root_hash, val, subtree.is_leaf())?)
    }
    Ok(subtrees_roots)
}
