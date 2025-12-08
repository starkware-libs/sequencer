use std::collections::HashMap;
use std::future::Future;

use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbKey, DbValue, Storage};

use crate::block_committer::input::{ConfigImpl, StarknetStorageValue};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub enum ForestMetadataType {
    CommitmentOffset,
    StateDiffHash(BlockNumber),
}

pub trait ForestMetadata {
    /// Returns the db key for the metadata type.
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey;

    /// Reads the metadata from the storage.
    fn read_metadata(
        &self,
        storage: &mut impl Storage,
        metadata_type: ForestMetadataType,
    ) -> impl Future<Output = ForestResult<Option<DbValue>>> + Send {
        async {
            let db_key = Self::metadata_key(metadata_type);
            Ok(storage.get(&db_key).await?)
        }
    }

    /// Sets the metadata in the storage. Returns the previous value if it existed.
    fn set_metadata(
        updates: &mut DbHashMap,
        metadata_type: ForestMetadataType,
        value: DbValue,
    ) -> Option<DbValue> {
        let db_key = Self::metadata_key(metadata_type);
        updates.insert(db_key, value)
    }
}

/// Trait for reading an original skeleton forest from some storage.
/// The implementation may depend on the underlying storage layout.
pub trait ForestReader<'a> {
    fn read(
        &mut self,
        contracts_trie_root_hash: HashOutput,
        classes_trie_root_hash: HashOutput,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        // TODO(Yoav): Change to 'impl Config' or delete this trait
        config: ConfigImpl,
    ) -> impl Future<
        Output = ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>,
    > + Send;
}

pub trait ForestWriter: ForestMetadata + Send {
    /// Serializes a filled forest into a hash map.
    fn serialize_forest(filled_forest: &FilledForest) -> DbHashMap;

    fn get_storage(&mut self) -> &mut impl Storage;

    /// Writes the serialized filled forest to storage. Returns the number of new updates written to
    /// storage.
    fn write(&mut self, filled_forest: &FilledForest) -> impl Future<Output = usize> + Send {
        async {
            let updates = Self::serialize_forest(filled_forest);
            self.write_updates(updates).await
        }
    }

    fn write_with_metadata(
        &mut self,
        filled_forest: &FilledForest,
        metadata: HashMap<ForestMetadataType, DbValue>,
    ) -> impl Future<Output = usize> + Send {
        async {
            let mut updates = Self::serialize_forest(filled_forest);
            for (metadata_type, value) in metadata {
                Self::set_metadata(&mut updates, metadata_type, value);
            }
            self.write_updates(updates).await
        }
    }

    fn write_updates(&mut self, updates: DbHashMap) -> impl Future<Output = usize> + Send {
        async {
            let n_updates = updates.len();
            self.get_storage()
                .mset(updates)
                .await
                .unwrap_or_else(|_| panic!("Write of {n_updates} new updates to storage failed"));
            n_updates
        }
    }
}
