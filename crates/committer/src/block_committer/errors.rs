use thiserror::Error;

use crate::{forest_errors::ForestError, patricia_merkle_tree::node_data::leaf::LeafData};

#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum BlockCommitmentError<L: LeafData> {
    #[error(transparent)]
    ForestError(#[from] ForestError<L>),
}
