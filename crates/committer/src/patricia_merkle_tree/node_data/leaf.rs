use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::Arc;

use crate::block_committer::input::StarknetStorageValue;
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::storage::db_object::{DBObject, Deserializable};

pub trait Leaf: Clone + Sync + Send + DBObject + Deserializable + Default + Debug + Eq {
    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;

    /// Creates a leaf.
    /// This function is async to allow computation of a leaf on the fly; in simple cases, it can
    /// be enough to return the leaf data directly using [Self::from_modifications].
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create(
        index: &NodeIndex,
        leaf_modifications: Arc<LeafModifications<Self>>,
    ) -> impl Future<Output = LeafResult<Self>> + Send;

    /// Extracts the leaf data from the leaf modifications. Returns an error if the leaf data is
    /// missing.
    fn from_modifications(
        index: &NodeIndex,
        leaf_modifications: Arc<LeafModifications<Self>>,
    ) -> LeafResult<Self> {
        let leaf_data = leaf_modifications
            .get(index)
            .ok_or(LeafError::MissingLeafModificationData(*index))?
            .clone();
        Ok(leaf_data)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

impl ContractState {
    // TODO(Aner, 11/4/24): Verify the correctness of the implementation.
    pub const CONTRACT_STATE_HASH_VERSION: Felt = Felt::ZERO;
}

impl Leaf for StarknetStorageValue {
    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(
        index: &NodeIndex,
        leaf_modifications: Arc<LeafModifications<Self>>,
    ) -> LeafResult<Self> {
        Self::from_modifications(index, leaf_modifications)
    }
}

impl Leaf for CompiledClassHash {
    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(
        index: &NodeIndex,
        leaf_modifications: Arc<LeafModifications<Self>>,
    ) -> LeafResult<Self> {
        Self::from_modifications(index, leaf_modifications)
    }
}

impl Leaf for ContractState {
    fn is_empty(&self) -> bool {
        self.nonce.0 == Felt::ZERO
            && self.class_hash.0 == Felt::ZERO
            && self.storage_root_hash.0 == Felt::ZERO
    }

    async fn create(
        index: &NodeIndex,
        leaf_modifications: Arc<LeafModifications<Self>>,
    ) -> LeafResult<Self> {
        Self::from_modifications(index, leaf_modifications)
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
