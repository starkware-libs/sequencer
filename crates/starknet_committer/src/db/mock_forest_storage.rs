use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::original_skeleton_tree::tree::OriginalSkeletonTreeImpl;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{
    create_db_key,
    DbHashMap,
    DbKey,
    DbKeyPrefix,
    DbValue,
    Storage,
};

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::db::forest_trait::{ForestMetadata, ForestMetadataType, ForestReader, ForestWriter};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub struct MockIndexInitialRead {}

impl InputContext for MockIndexInitialRead {}

// TODO(Yoav): Remove this once we have a real storage implementation.
pub struct MockForestStorage<S: Storage> {
    pub storage: S,
}

#[async_trait]
impl<S: Storage> ForestMetadata for MockForestStorage<S> {
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey {
        match metadata_type {
            ForestMetadataType::CommitmentOffset => DbKey("commitment_offset".into()),
            ForestMetadataType::StateDiffHash(block_number) => create_db_key(
                DbKeyPrefix::new(b"state_diff_hash".into()),
                &block_number.serialize(),
            ),
            ForestMetadataType::StateRoot(block_number) => {
                create_db_key(DbKeyPrefix::new(b"state_root".into()), &block_number.serialize())
            }
        }
    }

    async fn get_from_storage(&mut self, db_key: DbKey) -> ForestResult<Option<DbValue>> {
        Ok(self.storage.get(&db_key).await?)
    }
}

#[async_trait]
impl<S: Storage> ForestReader<MockIndexInitialRead> for MockForestStorage<S> {
    async fn read<'a>(
        &mut self,
        _context: MockIndexInitialRead,
        _storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        _classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        _config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        Ok((
            OriginalSkeletonForest {
                classes_trie: OriginalSkeletonTreeImpl::create_empty(
                    forest_sorted_indices.classes_trie_sorted_indices,
                ),
                contracts_trie: OriginalSkeletonTreeImpl::create_empty(
                    forest_sorted_indices.contracts_trie_sorted_indices,
                ),
                storage_tries: HashMap::new(),
            },
            HashMap::new(),
        ))
    }
}

#[async_trait]
impl<S: Storage> ForestWriter for MockForestStorage<S> {
    fn serialize_forest(_filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        Ok(HashMap::new())
    }

    async fn write_updates(&mut self, updates: DbHashMap) -> usize {
        let n_updates = updates.len();
        self.storage.mset(updates).await.expect("Write of updates to storage failed");
        n_updates
    }
}
