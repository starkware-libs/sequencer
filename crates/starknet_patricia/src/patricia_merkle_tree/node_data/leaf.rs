use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;

use crate::felt::Felt;
use crate::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use crate::patricia_merkle_tree::original_skeleton_tree::errors::OriginalSkeletonTreeError;
use crate::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeResult;
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::db_object::{DBObject, Deserializable};

pub trait Leaf: Clone + Sync + Send + DBObject + Deserializable + Default + Debug + Eq {
    // TODO(Amos, 1/1/2025): When default values for associated types are stable - use them, and
    // add a default implementation for `create`.
    // [Issue](https://github.com/rust-lang/rust/issues/29661).
    type Input: Send + Sync + 'static;
    type Output: Send + Debug + 'static;

    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;

    /// Creates a leaf. Allows returning additional output.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create(input: Self::Input) -> impl Future<Output = LeafResult<(Self, Self::Output)>> + Send;

    /// Extracts the leaf data from the leaf modifications. Returns an error if the leaf data is
    /// missing.
    fn from_modifications(
        index: &NodeIndex,
        leaf_modifications: &LeafModifications<Self>,
    ) -> LeafResult<Self> {
        let leaf_data = leaf_modifications
            .get(index)
            .ok_or(LeafError::MissingLeafModificationData(*index))?
            .clone();
        Ok(leaf_data)
    }

    /// Compares the previous leaf to the modified and returns true if they are equal.
    fn compare(
        leaf_modifications: &LeafModifications<Self>,
        index: &NodeIndex,
        previous_leaf: &Self,
    ) -> OriginalSkeletonTreeResult<bool> {
        let new_leaf = leaf_modifications
            .get(index)
            .ok_or(OriginalSkeletonTreeError::ReadModificationsError(*index))?;
        Ok(new_leaf == previous_leaf)
    }
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
        if value == Felt::ZERO { Self::Zero } else { Self::NonZero }
    }
}

pub type LeafModifications<L> = HashMap<NodeIndex, L>;
