use starknet_api::core::{ClassHash, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::{FilledTree, FilledTreeImpl};
use starknet_patricia::patricia_merkle_tree::node_data::errors::{LeafError, LeafResult};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::tree::UpdatedSkeletonTreeImpl;
use starknet_patricia_storage::db_object::HasStaticPrefix;
use starknet_patricia_storage::storage_trait::DbKeyPrefix;
use starknet_types_core::felt::Felt;

use super::leaf_serde::CommitterLeafPrefix;
use crate::block_committer::input::StarknetStorageValue;
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::types::CompiledClassHash;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContractState {
    pub nonce: Nonce,
    pub storage_root_hash: HashOutput,
    pub class_hash: ClassHash,
}

impl HasStaticPrefix for StarknetStorageValue {
    fn get_static_prefix() -> DbKeyPrefix {
        CommitterLeafPrefix::StorageLeaf.into()
    }
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

impl HasStaticPrefix for CompiledClassHash {
    fn get_static_prefix() -> DbKeyPrefix {
        CommitterLeafPrefix::CompiledClassLeaf.into()
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

impl HasStaticPrefix for ContractState {
    fn get_static_prefix() -> DbKeyPrefix {
        CommitterLeafPrefix::StateTreeLeaf.into()
    }
}

impl Leaf for ContractState {
    type Input = ContractStateInput;
    type Output = FilledTreeImpl<StarknetStorageValue>;

    fn is_empty(&self) -> bool {
        self.nonce.0 == Felt::ZERO
            && self.class_hash.0 == Felt::ZERO
            && self.storage_root_hash.0 == Felt::ZERO
    }

    async fn create(input: Self::Input) -> LeafResult<(Self, Self::Output)> {
        let ContractStateInput { leaf_index, nonce, class_hash, updated_skeleton, storage_updates } =
            input;

        let storage_trie = FilledTreeImpl::<StarknetStorageValue>::create_with_existing_leaves::<
            TreeHashFunctionImpl,
        >(updated_skeleton, storage_updates)
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

pub struct ContractStateInput {
    pub leaf_index: NodeIndex,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub updated_skeleton: UpdatedSkeletonTreeImpl,
    pub storage_updates: LeafModifications<StarknetStorageValue>,
}
