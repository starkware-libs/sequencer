use starknet_patricia::felt::Felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use starknet_patricia::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;

use crate::block_committer::input::StarknetStorageValue;
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::types::{ClassHash, CompiledClassHash, Nonce};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

impl Leaf for StarknetStorageValue {
    type Input = Self;
    type Output = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        Ok((input, ()))
    }
}

impl Leaf for CompiledClassHash {
    type Input = Self;
    type Output = ();

    fn is_empty(&self) -> bool {
        self.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        Ok((input, ()))
    }
}

impl Leaf for ContractState {
    type Input = (
        NodeIndex,
        Nonce,
        ClassHash,
        UpdatedSkeletonTreeImpl,
        LeafModifications<StarknetStorageValue>,
    );
    type Output = FilledTreeImpl<StarknetStorageValue>;

    fn is_empty(&self) -> bool {
        self.nonce.0 == Felt::ZERO
            && self.class_hash.0 == Felt::ZERO
            && self.storage_root_hash.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        let (leaf_index, nonce, class_hash, updated_skeleton, storage_modifications) = input;

        let storage_trie = FilledTreeImpl::<StarknetStorageValue>::create_with_existing_leaves::<
            TreeHashFunctionImpl,
        >(updated_skeleton, storage_modifications)
        .await
        .map_err(|storage_error| {
            LeafError::LeafComputationError(format!(
                "Creating a storage trie at index {:?} failed with the following error {:?}",
                leaf_index,
                storage_error.to_string()
            ))
        })?;
        Ok((
            Self { nonce, storage_root_hash: storage_trie.get_root_hash(), class_hash },
            storage_trie,
        ))
    }
}
