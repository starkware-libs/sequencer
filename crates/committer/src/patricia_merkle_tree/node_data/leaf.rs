use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::errors::LeafResult;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::db_object::{DBObject, Deserializable};
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;

pub trait Leaf: Clone + Sync + Send + DBObject + Deserializable + Default + Debug + Eq {
    // TODO(Amos, 1/1/2025): Add default values when it is stable.
    type I: Send + Sync + 'static;
    type O: Send + Debug + 'static;

    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;

    /// Creates a leaf.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create(input: Self::I) -> impl Future<Output = LeafResult<(Self, Option<Self::O>)>> + Send;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SkeletonLeaf {
    Zero,
    NonZero,
}

impl SkeletonLeaf {
    pub(crate) fn is_zero(&self) -> bool {
        self == &Self::Zero
    }
}

impl From<Felt> for SkeletonLeaf {
    fn from(value: Felt) -> Self {
        if value == Felt::ZERO {
            Self::Zero
        } else {
            Self::NonZero
        }
    }
}

pub type LeafModifications<L> = HashMap<NodeIndex, L>;
