use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_api::hash::StateRoots;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::db_object::EmptyKeyContext;
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{
    DbHashMap,
    DbKey,
    DbOperation,
    DbOperationMap,
    DbValue,
    PatriciaStorageResult,
    Storage,
};

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
use crate::db::serde_db_utils::DbBlockNumber;
use crate::db::trie_traversal::{create_classes_trie, create_contracts_trie, create_storage_tries};
use crate::forest::deleted_nodes::DeletedNodes;
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub enum ForestMetadataType {
    CommitmentOffset,
    StateDiffHash(DbBlockNumber),
    StateRoot(DbBlockNumber),
}

#[async_trait]
pub trait ForestMetadata {
    /// Returns the db key for the metadata type.
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey;

    /// Reads a value from the storage.
    async fn get_from_storage(&mut self, db_key: DbKey) -> ForestResult<Option<DbValue>>;

    /// Reads the metadata from the storage.
    async fn read_metadata(
        &mut self,
        metadata_type: ForestMetadataType,
    ) -> ForestResult<Option<DbValue>> {
        let db_key = Self::metadata_key(metadata_type);
        self.get_from_storage(db_key).await
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
pub trait ForestReader {
    /// Input required to start reading the storage trie.
    type InitialReadContext: InputContext + Send;

    async fn read<'a>(
        &mut self,
        roots: StateRoots,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>;

    async fn read_roots(
        &mut self,
        initial_read_context: Self::InitialReadContext,
    ) -> PatriciaStorageResult<StateRoots>;
}

/// Helper function containing layout-common read logic.
pub(crate) async fn read_forest<'a, S, Layout>(
    storage: &mut S,
    roots: StateRoots,
    storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    classes_updates: &'a LeafModifications<CompiledClassHash>,
    forest_sorted_indices: &'a ForestSortedIndices<'a>,
    config: ReaderConfig,
) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>
where
    S: Storage,
    Layout: DbLayout,
{
    let (contracts_trie, original_contracts_trie_leaves) =
        create_contracts_trie::<Layout::NodeLayout>(
            storage,
            roots.contracts_trie_root_hash,
            forest_sorted_indices.contracts_trie_sorted_indices,
        )
        .await?;
    let storage_tries = create_storage_tries::<Layout::NodeLayout>(
        storage,
        storage_updates,
        &original_contracts_trie_leaves,
        &config,
        &forest_sorted_indices.storage_tries_sorted_indices,
    )
    .await?;
    let classes_trie = create_classes_trie::<Layout::NodeLayout>(
        storage,
        classes_updates,
        roots.classes_trie_root_hash,
        &config,
        forest_sorted_indices.classes_trie_sorted_indices,
    )
    .await?;

    Ok((
        OriginalSkeletonForest { classes_trie, contracts_trie, storage_tries },
        original_contracts_trie_leaves,
    ))
}

/// Helper function containing layout-common write logic.
pub(crate) fn serialize_forest<Layout: DbLayout>(
    filled_forest: &FilledForest,
) -> SerializationResult<DbHashMap> {
    let mut serialized_forest = DbHashMap::new();

    // Storage tries.
    for (contract_address, tree) in &filled_forest.storage_tries {
        serialized_forest.extend(tree.serialize::<Layout::NodeLayout>(contract_address)?);
    }

    // Contracts trie.
    serialized_forest
        .extend(filled_forest.contracts_trie.serialize::<Layout::NodeLayout>(&EmptyKeyContext)?);

    // Classes trie.
    serialized_forest
        .extend(filled_forest.classes_trie.serialize::<Layout::NodeLayout>(&EmptyKeyContext)?);

    Ok(serialized_forest)
}

pub(crate) fn updates_to_set_operations(updates: DbHashMap) -> DbOperationMap {
    updates.into_iter().map(|(key, value)| (key, DbOperation::Set(value))).collect()
}

#[async_trait]
pub trait ForestWriter: Send {
    /// Serializes a filled forest into a hash map.
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap>;

    /// Writes the updates map to storage. Returns the number of new updates written to storage.
    async fn write_updates(&mut self, updates: DbOperationMap) -> usize;

    /// Writes the serialized filled forest to storage. Returns the number of new updates written to
    /// storage.
    async fn write(&mut self, filled_forest: &FilledForest) -> SerializationResult<usize> {
        let updates = Self::serialize_forest(filled_forest)?;
        Ok(self.write_updates(updates_to_set_operations(updates)).await)
    }
}

#[async_trait]
pub trait ForestWriterWithMetadata: ForestWriter + ForestMetadata {
    /// Serializes deleted nodes into a vector of database keys.
    fn serialize_deleted_nodes(deleted_nodes: &DeletedNodes) -> SerializationResult<Vec<DbKey>>;

    async fn write_with_metadata(
        &mut self,
        filled_forest: &FilledForest,
        metadata: HashMap<ForestMetadataType, DbValue>,
        deleted_nodes: &DeletedNodes,
    ) -> SerializationResult<usize> {
        let mut updates = Self::serialize_forest(filled_forest)?;
        for (metadata_type, value) in metadata {
            Self::insert_metadata(&mut updates, metadata_type, value);
        }
        let keys_to_delete = Self::serialize_deleted_nodes(deleted_nodes)?;
        let operations = keys_to_delete
            .into_iter()
            .map(|key| (key, DbOperation::Delete))
            .chain(updates_to_set_operations(updates))
            .collect();
        Ok(self.write_updates(operations).await)
    }
}

pub trait StorageInitializer {
    type Storage: Storage;
    fn new(storage: Self::Storage) -> Self;
}

pub trait ForestStorage: ForestReader + ForestWriterWithMetadata + StorageInitializer {}

impl<T: ForestReader + ForestWriterWithMetadata + StorageInitializer> ForestStorage for T {}

/// Trait for initial read contexts that can be created without external input.
pub trait EmptyInitialReadContext: InputContext {
    fn create_empty() -> Self;
}

/// ForestReader with empty initial read context.
pub trait ForestReaderWithEmptyContext:
    ForestReader<InitialReadContext: EmptyInitialReadContext>
{
}

impl<T> ForestReaderWithEmptyContext for T where
    T: ForestReader<InitialReadContext: EmptyInitialReadContext>
{
}

/// Marker trait for storage types that can initialize their read context without external input.
///
/// Types that require external context (e.g., `FactsDb` which needs roots provided externally as
/// they are not part of the committer storage) should NOT implement this trait.
pub trait ForestStorageWithEmptyReadContext:
    ForestReaderWithEmptyContext + ForestWriterWithMetadata + StorageInitializer
{
}

impl<T> ForestStorageWithEmptyReadContext for T where
    T: ForestReaderWithEmptyContext + ForestWriterWithMetadata + StorageInitializer
{
}
