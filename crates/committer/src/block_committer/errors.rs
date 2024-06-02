use thiserror::Error;

use crate::{forest_errors::ForestError, patricia_merkle_tree::node_data::leaf::LeafData};

#[derive(Debug, Error)]
pub enum BlockCommitmentError<L: LeafData> {
    #[error(transparent)]
    ForestError(#[from] ForestError<L>),
}
