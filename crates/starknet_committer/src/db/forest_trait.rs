use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbKey, DbValue, Storage};

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub enum ForestMetadataType {
    CommitmentOffset,
    StateDiffHash(BlockNumber),
    StateRoot(BlockNumber),
}

#[async_trait]
pub trait ForestMetadata {
    /// Returns the db key for the metadata type.
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey;

    /// Reads the metadata from the storage.
    async fn read_metadata(
        &self,
        storage: &mut impl Storage,
        metadata_type: ForestMetadataType,
    ) -> ForestResult<Option<DbValue>> {
        let db_key = Self::metadata_key(metadata_type);
        Ok(storage.get(&db_key).await?)
    }

    /// Adds the metadata to updates map. Returns the previous value if it existed.
    fn insert_metadata(
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
#[async_trait]
pub trait ForestReader<I: InputContext> {
    async fn read<'a>(
        &mut self,
        context: I,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>;
}

#[async_trait]
pub trait ForestWriter: Send {
    /// Serializes a filled forest into a hash map.
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap>;

    /// Writes the updates map to storage. Returns the number of new updates written to storage.
    async fn write_updates(&mut self, updates: DbHashMap) -> usize;

    /// Writes the serialized filled forest to storage. Returns the number of new updates written to
    /// storage.
    async fn write(&mut self, filled_forest: &FilledForest) -> SerializationResult<usize> {
        let updates = Self::serialize_forest(filled_forest)?;
        Ok(self.write_updates(updates).await)
    }
}

#[async_trait]
pub trait ForestWriterWithMetadata: ForestWriter + ForestMetadata {
    async fn write_with_metadata(
        &mut self,
        filled_forest: &FilledForest,
        metadata: HashMap<ForestMetadataType, DbValue>,
    ) -> SerializationResult<usize> {
        let mut updates = Self::serialize_forest(filled_forest)?;
        for (metadata_type, value) in metadata {
            Self::insert_metadata(&mut updates, metadata_type, value);
        }
        Ok(self.write_updates(updates).await)
    }
}

impl<T: ForestWriter + ForestMetadata> ForestWriterWithMetadata for T {}

pub trait ForestStorage<I: InputContext>: ForestReader<I> + ForestWriterWithMetadata {}
impl<I: InputContext, T: ForestReader<I> + ForestWriterWithMetadata> ForestStorage<I> for T {}
