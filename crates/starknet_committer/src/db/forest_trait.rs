use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::core::ContractAddress;
use starknet_patricia::db_layout::NodeLayoutFor;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTree;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::db_object::{EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbKey, DbValue, Storage};

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::db::facts_db::types::FactsDbInitialRead;
use crate::db::serde_db_utils::DbBlockNumber;
use crate::db::trie_traversal::{create_classes_trie, create_contracts_trie, create_storage_tries};
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

/// Helper function containing layout-common read logic.
pub(crate) async fn read_forest<'a, S, Layout>(
    storage: &mut S,
    context: FactsDbInitialRead,
    storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    classes_updates: &'a LeafModifications<CompiledClassHash>,
    forest_sorted_indices: &'a ForestSortedIndices<'a>,
    config: ReaderConfig,
) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)>
where
    S: Storage,
    Layout: NodeLayoutFor<StarknetStorageValue>
        + NodeLayoutFor<ContractState>
        + NodeLayoutFor<CompiledClassHash>,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
    <Layout as NodeLayoutFor<ContractState>>::DbLeaf: HasStaticPrefix<KeyContext = EmptyKeyContext>,
    <Layout as NodeLayoutFor<CompiledClassHash>>::DbLeaf:
        HasStaticPrefix<KeyContext = EmptyKeyContext>,
{
    let (contracts_trie, original_contracts_trie_leaves) = create_contracts_trie::<Layout>(
        storage,
        context.0.contracts_trie_root_hash,
        forest_sorted_indices.contracts_trie_sorted_indices,
    )
    .await?;
    let storage_tries = create_storage_tries::<Layout>(
        storage,
        storage_updates,
        &original_contracts_trie_leaves,
        &config,
        &forest_sorted_indices.storage_tries_sorted_indices,
    )
    .await?;
    let classes_trie = create_classes_trie::<Layout>(
        storage,
        classes_updates,
        context.0.classes_trie_root_hash,
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
pub(crate) fn serialize_forest<Layout>(
    filled_forest: &FilledForest,
) -> SerializationResult<DbHashMap>
where
    Layout: NodeLayoutFor<StarknetStorageValue>
        + NodeLayoutFor<ContractState>
        + NodeLayoutFor<CompiledClassHash>,
    <Layout as NodeLayoutFor<StarknetStorageValue>>::DbLeaf:
        HasStaticPrefix<KeyContext = ContractAddress>,
    <Layout as NodeLayoutFor<ContractState>>::DbLeaf: HasStaticPrefix<KeyContext = EmptyKeyContext>,
    <Layout as NodeLayoutFor<CompiledClassHash>>::DbLeaf:
        HasStaticPrefix<KeyContext = EmptyKeyContext>,
{
    let mut serialized_forest = DbHashMap::new();

    // Storage tries.
    for (contract_address, tree) in &filled_forest.storage_tries {
        serialized_forest.extend(tree.serialize::<Layout>(&contract_address)?);
    }

    // Contracts trie.
    serialized_forest.extend(filled_forest.contracts_trie.serialize::<Layout>(&EmptyKeyContext)?);

    // Classes trie.
    serialized_forest.extend(filled_forest.classes_trie.serialize::<Layout>(&EmptyKeyContext)?);

    Ok(serialized_forest)
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
