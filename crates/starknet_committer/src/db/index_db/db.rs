use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::LazyLock;

use async_trait::async_trait;
#[cfg(feature = "os_input")]
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PATRICIA_KEY_UPPER_BOUND_FELT};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_patricia::db_layout::{NodeLayout, NodeLayoutFor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::{Leaf, LeafModifications};
#[cfg(feature = "os_input")]
use starknet_patricia::patricia_merkle_tree::traversal::TraversalResult;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;
use starknet_patricia_storage::db_object::{DBObject, EmptyKeyContext, HasStaticPrefix};
use starknet_patricia_storage::errors::{DeserializationError, SerializationResult};
#[cfg(feature = "os_input")]
use starknet_patricia_storage::map_storage::MapStorage;
#[cfg(any(feature = "testing", test))]
use starknet_patricia_storage::storage_trait::AsyncStorage;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::DbOperation;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::ImmutableReadOnlyStorage;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::PatriciaStorageError;
use starknet_patricia_storage::storage_trait::{
    DbHashMap,
    DbKey,
    DbOperationMap,
    DbValue,
    PatriciaStorageResult,
    Storage,
};
#[cfg(feature = "os_input")]
use starknet_patricia_storage::two_layer_storage::TwoLayerStorage;
use starknet_types_core::felt::Felt;

use crate::block_committer::input::{InputContext, ReaderConfig, StarknetStorageValue};
use crate::db::db_layout::DbLayout;
#[cfg(feature = "os_input")]
use crate::db::forest_trait::forest_trait_witnesses::{
    CommitmentInfosUpdate,
    CommitmentInfosWrite,
    ForestReaderWithWitnesses,
    ForestWriterWithMetadataAndWitnesses,
};
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
};
use crate::db::index_db::types::{
    get_node_index_db_key,
    EmptyNodeData,
    IndexFilledNode,
    IndexFilledNodeWithHasher,
    IndexLayoutSubTree,
    IndexNodeContext,
};
use crate::db::serde_db_utils::DbBlockNumber;
use crate::forest::deleted_nodes::DeletedNodes;
use crate::forest::filled_forest::FilledForest;
#[cfg(feature = "os_input")]
use crate::forest::forest_errors::ForestError;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
#[cfg(feature = "os_input")]
use crate::patricia_merkle_tree::tree::{fetch_all_patricia_paths, SortedLeafIndices};
use crate::patricia_merkle_tree::types::CompiledClassHash;
#[cfg(feature = "os_input")]
use crate::patricia_merkle_tree::types::{StarknetForestProofs, StateCommitmentInfos};

/// Set to 2^251 + 1 to avoid collisions with contract addresses prefixes.
pub(crate) static FIRST_AVAILABLE_PREFIX_FELT: LazyLock<Felt> =
    LazyLock::new(|| PATRICIA_KEY_UPPER_BOUND_FELT + Felt::ONE);

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

/// Prefix for accessed-keys digest metadata (committed per block).
pub(crate) static ACCESSED_KEYS_DIGEST_METADATA_PREFIX: LazyLock<[u8; 32]> =
    LazyLock::new(|| (Felt::from_bytes_be(&STATE_ROOT_METADATA_PREFIX) + Felt::ONE).to_bytes_be());

/// Prefix for Patricia proofs payload (per block).
#[cfg_attr(not(feature = "os_input"), expect(dead_code))]
pub(crate) static PATRICIA_PATHS_PREFIX: LazyLock<[u8; 32]> = LazyLock::new(|| {
    (Felt::from_bytes_be(&ACCESSED_KEYS_DIGEST_METADATA_PREFIX) + Felt::ONE).to_bytes_be()
});

pub struct IndexDb<S: Storage, H = TreeHashFunctionImpl> {
    storage: S,
    phantom: PhantomData<H>,
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

impl<S: Storage, H> StorageInitializer for IndexDb<S, H> {
    type Storage = S;
    fn new(storage: Self::Storage) -> Self {
        Self { storage, phantom: PhantomData }
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

pub struct IndexNodeLayout<H = TreeHashFunctionImpl>(PhantomData<H>);

pub trait IndexLayoutHasher:
    TreeHashFunction<IndexLayoutStarknetStorageValue>
    + TreeHashFunction<IndexLayoutContractState>
    + TreeHashFunction<IndexLayoutCompiledClassHash>
{
}

impl<H> IndexLayoutHasher for H where
    H: TreeHashFunction<IndexLayoutStarknetStorageValue>
        + TreeHashFunction<IndexLayoutContractState>
        + TreeHashFunction<IndexLayoutCompiledClassHash>
{
}

impl<'a, L, H> NodeLayout<'a, L> for IndexNodeLayout<H>
where
    L: Leaf,
    H: TreeHashFunction<L>,
{
    type NodeData = EmptyNodeData;
    type NodeDbObject = IndexFilledNodeWithHasher<L, H>;
    type DeserializationContext = IndexNodeContext;
    type SubTree = IndexLayoutSubTree<'a>;

    fn get_db_object<LeafBase: Leaf + Into<L>>(
        node_index: NodeIndex,
        key_context: &<L as HasStaticPrefix>::KeyContext,
        filled_node: FilledNode<LeafBase, HashOutput>,
    ) -> (DbKey, Self::NodeDbObject) {
        let filled_node = Self::convert_node_data_and_leaf(filled_node);

        let db_filled_node = IndexFilledNodeWithHasher::<L, H>::new(filled_node);

        let suffix = &node_index.0.to_be_bytes();
        let key = db_filled_node.get_db_key(key_context, suffix);

        (key, db_filled_node)
    }
}

impl<H> NodeLayoutFor<StarknetStorageValue> for IndexNodeLayout<H>
where
    H: TreeHashFunction<IndexLayoutStarknetStorageValue>,
{
    type DbLeaf = IndexLayoutStarknetStorageValue;
}

impl<H> NodeLayoutFor<ContractState> for IndexNodeLayout<H>
where
    H: TreeHashFunction<IndexLayoutContractState>,
{
    type DbLeaf = IndexLayoutContractState;
}

impl<H> NodeLayoutFor<CompiledClassHash> for IndexNodeLayout<H>
where
    H: TreeHashFunction<IndexLayoutCompiledClassHash>,
{
    type DbLeaf = IndexLayoutCompiledClassHash;
}

impl<H> DbLayout for IndexNodeLayout<H>
where
    H: IndexLayoutHasher,
{
    type ContractStateDbLeaf = IndexLayoutContractState;
    type CompiledClassHashDbLeaf = IndexLayoutCompiledClassHash;
    type StarknetStorageValueDbLeaf = IndexLayoutStarknetStorageValue;
    type NodeLayout = IndexNodeLayout<H>;
}

// TODO(Ariel): define an IndexDbInitialRead empty type, and check whether each tree is empty inside
// create_xxx_trie.
#[async_trait]
impl<S: Storage, H: Send + 'static> ForestReader for IndexDb<S, H>
where
    H: IndexLayoutHasher,
{
    type InitialReadContext = IndexDbReadContext;

    /// Creates an original skeleton forest that includes the storage tries of the modified
    /// contracts, the classes trie and the contracts trie. Additionally, returns the original
    /// contract states that are needed to compute the contract state tree.
    async fn read<'a>(
        &mut self,
        roots: StateRoots,
        storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
        classes_updates: &LeafModifications<CompiledClassHash>,
        forest_sorted_indices: &'a ForestSortedIndices<'a>,
        config: ReaderConfig,
    ) -> ForestResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
        read_forest::<S, IndexNodeLayout<H>>(
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
        let contracts_trie_root_key =
            get_node_index_db_key::<IndexLayoutContractState>(&EmptyKeyContext, NodeIndex::ROOT);
        let classes_trie_root_key = get_node_index_db_key::<IndexLayoutCompiledClassHash>(
            &EmptyKeyContext,
            NodeIndex::ROOT,
        );

        let roots =
            self.storage.mget_mut(&[&contracts_trie_root_key, &classes_trie_root_key]).await?;
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
        // Padding to 64byte keys to keep the 32byte prefix aligned between metadata and
        // patricia nodes.
        DbKey(match metadata_type {
            ForestMetadataType::CommitmentOffset => {
                let mut key = Vec::with_capacity(64);
                key.extend_from_slice(&*COMMITMENT_OFFSET_METADATA_PREFIX);
                key.extend_from_slice(&[0u8; 32]);
                key
            }
            ForestMetadataType::StateDiffHash(block_number) => {
                block_number_based_key(&STATE_DIFF_HASH_METADATA_PREFIX, block_number)
            }
            ForestMetadataType::StateRoot(block_number) => {
                block_number_based_key(&STATE_ROOT_METADATA_PREFIX, block_number)
            }
            #[cfg(feature = "os_input")]
            ForestMetadataType::AccessedKeysDigest(block_number) => {
                block_number_based_key(&ACCESSED_KEYS_DIGEST_METADATA_PREFIX, block_number)
            }
        })
    }

    async fn get_from_storage(&mut self, db_key: DbKey) -> ForestResult<Option<DbValue>> {
        Ok(self.storage.get_mut(&db_key).await?)
    }
}

impl<S: Storage> ForestWriterWithMetadata for IndexDb<S> {
    fn serialize_deleted_nodes(
        DeletedNodes { classes_trie, contracts_trie, storage_tries }: DeletedNodes,
    ) -> Vec<DbKey> {
        classes_trie
            .iter()
            .map(|node_index| {
                get_node_index_db_key::<IndexLayoutCompiledClassHash>(&EmptyKeyContext, *node_index)
            })
            .chain(contracts_trie.iter().map(|node_index| {
                get_node_index_db_key::<IndexLayoutContractState>(&EmptyKeyContext, *node_index)
            }))
            .chain(storage_tries.iter().flat_map(|(contract_address, node_indices)| {
                node_indices.iter().map(move |node_index| {
                    get_node_index_db_key::<IndexLayoutStarknetStorageValue>(
                        contract_address,
                        *node_index,
                    )
                })
            }))
            .collect()
    }
}

fn block_number_based_key(prefix: &[u8; 32], block_number: DbBlockNumber) -> Vec<u8> {
    let mut key = Vec::with_capacity(64);
    key.extend_from_slice(prefix);
    key.extend_from_slice(&block_number.serialize());
    key.extend_from_slice(&[0u8; 24]);
    key
}

#[cfg(feature = "os_input")]
#[async_trait]
impl<S: Storage + ImmutableReadOnlyStorage + Sync + Send + 'static> ForestReaderWithWitnesses
    for IndexDb<S>
{
    async fn read_commitment_infos(
        &mut self,
        height: BlockNumber,
    ) -> ForestResult<Option<StateCommitmentInfos>> {
        let db_key = DbKey(block_number_based_key(&PATRICIA_PATHS_PREFIX, DbBlockNumber(height)));

        Ok(match self.get_from_storage(db_key).await? {
            None => None,
            Some(DbValue(bytes)) => {
                Some(StateCommitmentInfos::decompress(&bytes).map_err(|e| {
                    ForestError::PatriciaStorage(PatriciaStorageError::Deserialization(
                        DeserializationError::ValueError(Box::new(e)),
                    ))
                })?)
            }
        })
    }

    async fn fetch_patricia_witnesses(
        &mut self,
        classes_trie_root_hash: HashOutput,
        contracts_trie_root_hash: HashOutput,
        class_sorted_leaf_indices: SortedLeafIndices<'_>,
        contract_sorted_leaf_indices: SortedLeafIndices<'_>,
        contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
        staged_serialized_forest: Option<DbHashMap>,
    ) -> TraversalResult<StarknetForestProofs> {
        match staged_serialized_forest {
            None => {
                fetch_all_patricia_paths::<IndexNodeLayout>(
                    &mut self.storage,
                    classes_trie_root_hash,
                    contracts_trie_root_hash,
                    class_sorted_leaf_indices,
                    contract_sorted_leaf_indices,
                    contract_storage_sorted_leaf_indices,
                )
                .await
            }
            Some(modifications) => {
                let mut overlay = MapStorage::default();
                overlay.mset(modifications).await?;
                let mut layered = TwoLayerStorage::new(overlay, &self.storage);
                fetch_all_patricia_paths::<IndexNodeLayout>(
                    &mut layered,
                    classes_trie_root_hash,
                    contracts_trie_root_hash,
                    class_sorted_leaf_indices,
                    contract_sorted_leaf_indices,
                    contract_storage_sorted_leaf_indices,
                )
                .await
            }
        }
    }
}

#[cfg(feature = "os_input")]
#[async_trait]
impl<S: Storage + Send> ForestWriterWithMetadataAndWitnesses for IndexDb<S> {
    async fn write_with_metadata_and_commitment_infos(
        &mut self,
        filled_forest: &FilledForest,
        metadata: HashMap<ForestMetadataType, DbValue>,
        deleted_nodes: DeletedNodes,
        commitment_infos_update: CommitmentInfosUpdate,
    ) -> SerializationResult<usize> {
        let mut operations = DbOperationMap::new();
        Self::append_forest_and_metadata(&mut operations, filled_forest, metadata, deleted_nodes)?;
        match commitment_infos_update {
            CommitmentInfosUpdate::Delete(block_number) => {
                operations.insert(
                    Self::metadata_key(ForestMetadataType::AccessedKeysDigest(DbBlockNumber(
                        block_number,
                    ))),
                    DbOperation::Delete,
                );
                operations.insert(
                    DbKey(block_number_based_key(
                        &PATRICIA_PATHS_PREFIX,
                        DbBlockNumber(block_number),
                    )),
                    DbOperation::Delete,
                );
            }
            CommitmentInfosUpdate::Write(CommitmentInfosWrite {
                block_number,
                keys_digest,
                commitment_infos,
            }) => {
                let encoded = DbValue(commitment_infos.compress()?);
                operations.insert(
                    Self::metadata_key(ForestMetadataType::AccessedKeysDigest(DbBlockNumber(
                        block_number,
                    ))),
                    DbOperation::Set(DbValue(keys_digest.to_vec())),
                );
                operations.insert(
                    DbKey(block_number_based_key(
                        &PATRICIA_PATHS_PREFIX,
                        DbBlockNumber(block_number),
                    )),
                    DbOperation::Set(encoded),
                );
            }
        }
        Ok(self.write_updates(operations).await)
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

#[cfg(all(feature = "os_input", any(test, feature = "testing")))]
impl IndexDb<MapStorage> {
    /// Removes Patricia trie node keys while keeping commitment metadata and stored witness
    /// payloads. Tests can call this before replaying `read_paths_and_commit_block` to ensure
    /// the historical path reads persisted witnesses rather than re-fetching from tries.
    pub fn clear_patricia_trie_nodes_for_test(&mut self) {
        self.storage.0.retain(|key, _| {
            if key.0.len() < 32 {
                return false;
            }
            let prefix: &[u8; 32] =
                key.0[..32].try_into().expect("metadata key prefix must be 32 bytes");
            // Retain metadata keys only: all prefixes from commitment offset onward. The two
            // preceding reserved prefixes are the contracts and classes tries, which we skip.
            prefix >= &*COMMITMENT_OFFSET_METADATA_PREFIX
        });
    }
}
