use std::fmt::Debug;
use thiserror::Error;

use crate::patricia_merkle_tree::node_data::inner_node::EdgePathLength;

#[derive(Debug, Error)]
pub enum PathToBottomError {
    #[error("Tried to remove {n_edges:?} edges from a {length:?} length path.")]
    RemoveEdgesError {
        length: EdgePathLength,
        n_edges: EdgePathLength,
    },
}
