use crate::block_committer::input::StarknetStorageValue;
use crate::felt::Felt;
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_tree::node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use crate::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use crate::patricia_merkle_tree::types::NodeIndex;
use crate::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use crate::storage::db_object::{DBObject, Deserializable};
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;

pub trait Leaf: Clone + Sync + Send + DBObject + Deserializable + Default + Debug + Eq {
    // TODO(Amos, 1/1/2025): Add default values when it is stable.
    type I: Send + Sync + 'static;
    type O: Send + 'static;

    /// Returns true if leaf is empty.
    fn is_empty(&self) -> bool;

    /// Creates a leaf.
    // Use explicit desugaring of `async fn` to allow adding trait bounds to the return type, see
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#async-fn-in-public-traits
    // for details.
    fn create(input: Self::I) -> impl Future<Output = LeafResult<(Self, Option<Self::O>)>> + Send;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

impl Leaf for StarknetStorageValue {
    type I = Self;
    type O = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }
    async fn create(input: Self::I) -> LeafResult<(Self, Option<Self::O>)> {
        Ok((input, None))
    }
}

impl Leaf for CompiledClassHash {
    type I = Self;
    type O = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(input: Self::I) -> LeafResult<(Self, Option<Self::O>)> {
        Ok((input, None))
    }
}

impl Leaf for ContractState {
    type I = (
        NodeIndex,
        Nonce,
        ClassHash,
        UpdatedSkeletonTreeImpl,
        LeafModifications<StarknetStorageValue>,
    );
    type O = FilledTreeImpl<StarknetStorageValue>;

    fn is_empty(&self) -> bool {
        self.nonce.0 == Felt::ZERO
            && self.class_hash.0 == Felt::ZERO
            && self.storage_root_hash.0 == Felt::ZERO
    }

    async fn create(input: Self::I) -> LeafResult<(Self, Option<Self::O>)> {
        let (leaf_index, nonce, class_hash, updated_skeleton, storage_modifications) = input;

        match FilledTreeImpl::<StarknetStorageValue>::create::<TreeHashFunctionImpl>(
            updated_skeleton.into(),
            storage_modifications.into(),
        )
        .await
        {
            Ok((storage_trie, _)) => Ok((
                Self {
                    nonce,
                    storage_root_hash: storage_trie.get_root_hash(),
                    class_hash,
                },
                Some(storage_trie),
            )),
            Err(storage_error) => Err(LeafError::StorageTrieComputationFailed(
                storage_error.into(),
                leaf_index,
            )),
        }
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
