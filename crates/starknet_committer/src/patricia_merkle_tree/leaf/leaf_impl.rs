use crate::block_committer::input::StarknetStorageValue;
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::types::{ClassHash, CompiledClassHash, Nonce};
use committer::felt::Felt;
use committer::hash::hash_trait::HashOutput;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTree;
use committer::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use committer::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use committer::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use committer::patricia_merkle_tree::types::NodeIndex;
use committer::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

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

        match FilledTreeImpl::<StarknetStorageValue>::create_with_existing_leaves::<
            TreeHashFunctionImpl,
        >(updated_skeleton.into(), storage_modifications)
        .await
        {
            Ok(storage_trie) => Ok((
                Self { nonce, storage_root_hash: storage_trie.get_root_hash(), class_hash },
                Some(storage_trie),
            )),
            Err(storage_error) => {
                Err(LeafError::LeafComputationError(storage_error.to_string(), leaf_index))
            }
        }
    }
}
