use std::collections::HashMap;
use std::sync::LazyLock;

use async_trait::async_trait;
use starknet_api::core::{ContractAddress, PATRICIA_KEY_UPPER_BOUND};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::{DeserializationError, SerializationResult};
#[cfg(any(feature = "testing", test))]
use starknet_patricia_storage::storage_trait::AsyncStorage;
use starknet_patricia_storage::storage_trait::{
    create_db_key,
    DbHashMap,
    DbKey,
    DbKeyPrefix,
    DbOperationMap,
    DbValue,
    PatriciaStorageResult,
    Storage,
};
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
use crate::db::forest_trait::{
    read_forest,
    serialize_forest,
    EmptyInitialReadContext,
    ForestMetadata,
    ForestMetadataType,
    ForestReader,
    ForestWriter,
    ForestWriterWithMetadata,
    StorageInitializer,
};
use crate::db::index_db::leaves::{
    IndexLayoutCompiledClassHash,
    IndexLayoutContractState,
    IndexLayoutStarknetStorageValue,
    INDEX_LAYOUT_DB_KEY_SEPARATOR,
};
use crate::db::index_db::types::{
    EmptyNodeData,
    IndexFilledNode,
    IndexLayoutSubTree,
    IndexNodeContext,
};
use crate::forest::deleted_nodes::DeletedNodes;
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

/// Set to 2^251 + 1 to avoid collisions with contract addresses prefixes.
pub(crate) static FIRST_AVAILABLE_PREFIX_FELT: LazyLock<Felt> =
    LazyLock::new(|| Felt::from_hex_unchecked(PATRICIA_KEY_UPPER_BOUND) + Felt::ONE);

/// The db key prefix of nodes in the contracts trie.
pub(crate) static CONTRACTS_TREE_PREFIX: LazyLock<[u8; 32]> =
    LazyLock::new(|| FIRST_AVAILABLE_PREFIX_FELT.to_bytes_be());

/// The db key prefix of nodes in the contracts trie.
pub(crate) static CLASSES_TREE_PREFIX: LazyLock<[u8; 32]> =
    LazyLock::new(|| (Felt::from_bytes_be(&CONTRACTS_TREE_PREFIX) + Felt::ONE).to_bytes_be());

/// The db key prefix of the commitment offset.
static COMMITMENT_OFFSET_METADATA_PREFIX: LazyLock<[u8; 32]> =
    LazyLock::new(|| (Felt::from_bytes_be(&CLASSES_TREE_PREFIX) + Felt::ONE).to_bytes_be());

/// The db key prefix of the block number to state diff hash mapping.
static STATE_DIFF_HASH_METADATA_PREFIX: LazyLock<[u8; 32]> = LazyLock::new(|| {
    (Felt::from_bytes_be(&COMMITMENT_OFFSET_METADATA_PREFIX) + Felt::ONE).to_bytes_be()
});

/// The db key prefix of the block number to state root mapping.
static STATE_ROOT_METADATA_PREFIX: LazyLock<[u8; 32]> = LazyLock::new(|| {
    (Felt::from_bytes_be(&STATE_DIFF_HASH_METADATA_PREFIX) + Felt::ONE).to_bytes_be()
});

pub struct IndexDb<S: Storage> {
    storage: S,
}

impl<S: Storage> IndexDb<S> {
    pub fn get_stats(&self) -> PatriciaStorageResult<S::Stats> {
        self.storage.get_stats()
    }

    pub fn reset_stats(&mut self) -> PatriciaStorageResult<()> {
        self.storage.reset_stats()
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_async_underlying_storage<'a>(&'a self) -> Option<impl AsyncStorage + 'a> {
        self.storage.get_async_self()
    }
}

impl<S: Storage> StorageInitializer for IndexDb<S> {
    type Storage = S;
    fn new(storage: Self::Storage) -> Self {
        Self { storage }
    }
}

/// Empty initial context for index db. We don't need external information to start reading the
/// tries.
#[derive(Clone)]
pub struct IndexDbReadContext;

impl InputContext for IndexDbReadContext {}

impl EmptyInitialReadContext for IndexDbReadContext {
    fn create_empty() -> Self {
        Self
    }
}

pub struct IndexNodeLayout {}

impl<'a, L> NodeLayout<'a, L> for IndexNodeLayout
where
    L: Leaf,
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    type NodeData = EmptyNodeData;
    type NodeDbObject = IndexFilledNode<L>;
    type DeserializationContext = IndexNodeContext;
    type SubTree = IndexLayoutSubTree<'a>;

    fn get_db_object<LeafBase: Leaf + Into<L>>(
        node_index: NodeIndex,
        key_context: &<L as HasStaticPrefix>::KeyContext,
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> (DbKey, Self::NodeDbObject) {
        let filled_node = Self::convert_node_data_and_leaf(filled_node);

        let db_filled_node = IndexFilledNode(filled_node);

        let suffix = &node_index.0.to_be_bytes();
        let key = db_filled_node.get_db_key(key_context, suffix);

        (key, db_filled_node)
    }
}

impl NodeLayoutFor<StarknetStorageValue> for IndexNodeLayout {
    type DbLeaf = IndexLayoutStarknetStorageValue;
}

impl NodeLayoutFor<ContractState> for IndexNodeLayout {
    type DbLeaf = IndexLayoutContractState;
}

impl NodeLayoutFor<CompiledClassHash> for IndexNodeLayout {
    type DbLeaf = IndexLayoutCompiledClassHash;
}

impl DbLayout for IndexNodeLayout {
    type ContractStateDbLeaf = IndexLayoutContractState;
    type CompiledClassHashDbLeaf = IndexLayoutCompiledClassHash;
    type StarknetStorageValueDbLeaf = IndexLayoutStarknetStorageValue;
    type NodeLayout = IndexNodeLayout;
}

fn create_index_layout_db_key(prefix: DbKeyPrefix, node_index: NodeIndex) -> DbKey {
    let suffix = node_index.0.to_be_bytes();
    create_db_key(prefix, INDEX_LAYOUT_DB_KEY_SEPARATOR, &suffix)
}

// TODO(Ariel): define an IndexDbInitialRead empty type, and check whether each tree is empty inside
// create_xxx_trie.
#[async_trait]
impl<S: Storage> ForestReader for IndexDb<S> {
    type InitialReadContext = IndexDbReadContext;

    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read<'a>(
        &mut self,
        roots: StateRoots,
        storage_updates: &'a HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &'a LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        read_forest::<S, IndexNodeLayout>(
            &mut self.storage,
            roots,
            storage_updates,
            classes_updates,
            forest_sorted_indices,
            config,
        )
        .await
    }

    async fn read_roots(
        &mut self,
        _initial_read_context: Self::InitialReadContext,
    ) -> PatriciaStorageResult<StateRoots> {
        let contracts_trie_root_key = create_index_layout_db_key(
            IndexLayoutContractState::get_static_prefix(&EmptyKeyContext),
            NodeIndex::ROOT,
        );
        let classes_trie_root_key = create_index_layout_db_key(
            IndexLayoutCompiledClassHash::get_static_prefix(&EmptyKeyContext),
            NodeIndex::ROOT,
        );

        let roots = self.storage.mget(&[&contracts_trie_root_key, &classes_trie_root_key]).await?;
        let contracts_trie_root_hash = extract_root_hash::<IndexLayoutContractState>(&roots[0])?;
        let classes_trie_root_hash = extract_root_hash::<IndexLayoutCompiledClassHash>(&roots[1])?;

        Ok(StateRoots { contracts_trie_root_hash, classes_trie_root_hash })
    }
}

#[async_trait]
impl<S: Storage> ForestWriter for IndexDb<S> {
    fn serialize_forest(filled_forest: &FilledForest) -> SerializationResult<DbHashMap> {
        serialize_forest::<IndexNodeLayout>(filled_forest)
    }

    async fn write_updates(&mut self, updates: DbOperationMap) -> usize {
        let n_updates = updates.len();
        self.storage
            .multi_set_and_delete(updates)
            .await
            .unwrap_or_else(|_| panic!("Write of {n_updates} new updates to storage failed"));
        n_updates
    }
}

#[async_trait]
impl<S: Storage> ForestMetadata for IndexDb<S> {
    fn metadata_key(metadata_type: ForestMetadataType) -> DbKey {
        let mut key = Vec::with_capacity(64);
        match metadata_type {
            // Padding to 64byte keys to keep the 32byte prefix aligned between metadata and
            // patricia nodes.
            ForestMetadataType::CommitmentOffset => {
                key.extend_from_slice(&*COMMITMENT_OFFSET_METADATA_PREFIX);
                key.extend_from_slice(&[0u8; 32]);
            }
            ForestMetadataType::StateDiffHash(block_number) => {
                key.extend_from_slice(&*STATE_DIFF_HASH_METADATA_PREFIX);
                let block_number_bytes: [u8; 8] = block_number.serialize();
                key.extend_from_slice(&block_number_bytes);
                key.extend_from_slice(&[0u8; 24]);
            }
            ForestMetadataType::StateRoot(block_number) => {
                key.extend_from_slice(&*STATE_ROOT_METADATA_PREFIX);
                let block_number_bytes: [u8; 8] = block_number.serialize();
                key.extend_from_slice(&block_number_bytes);
                key.extend_from_slice(&[0u8; 24]);
            }
        }
        DbKey(key)
    }

    async fn get_from_storage(&mut self, db_key: DbKey) -> ForestResult<Option<DbValue>> {
        Ok(self.storage.get(&db_key).await?)
    }
}

impl<S: Storage> ForestWriterWithMetadata for IndexDb<S> {
    fn serialize_deleted_nodes(deleted_nodes: &DeletedNodes) -> SerializationResult<Vec<DbKey>> {
        let mut keys_to_delete = Vec::new();

        // Classes trie deleted nodes.
        for node_index in &deleted_nodes.classes_trie {
            let prefix = IndexLayoutCompiledClassHash::get_static_prefix(&EmptyKeyContext);
            keys_to_delete.push(create_index_layout_db_key(prefix, *node_index));
        }

        // Contracts trie deleted nodes.
        for node_index in &deleted_nodes.contracts_trie {
            let prefix = IndexLayoutContractState::get_static_prefix(&EmptyKeyContext);
            keys_to_delete.push(create_index_layout_db_key(prefix, *node_index));
        }

        // Storage tries deleted nodes.
        for (contract_address, node_indices) in &deleted_nodes.storage_tries {
            for node_index in node_indices {
                let prefix = IndexLayoutStarknetStorageValue::get_static_prefix(contract_address);
                keys_to_delete.push(create_index_layout_db_key(prefix, *node_index));
            }
        }

        Ok(keys_to_delete)
    }
}

fn extract_root_hash<L: Leaf>(root: &Option<DbValue>) -> Result<HashOutput, DeserializationError>
where
    TreeHashFunctionImpl: TreeHashFunction<L>,
{
    if let Some(root) = root {
        let root_node =
            IndexFilledNode::<L>::deserialize(root, &IndexNodeContext { is_leaf: false })?;
        Ok(root_node.0.hash)
    } else {
        Ok(HashOutput::ROOT_OF_EMPTY_TREE)
    }
}
