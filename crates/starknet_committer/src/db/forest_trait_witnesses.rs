use std::collections::HashMap;

use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::traversal::TraversalResult;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::errors::SerializationResult;
use starknet_patricia_storage::storage_trait::{DbHashMap, DbValue};

use super::{
    EmptyInitialReadContext,
    ForestMetadataType,
    ForestReader,
    ForestWriterWithMetadata,
    StorageInitializer,
};
use crate::forest::deleted_nodes::DeletedNodes;
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::patricia_merkle_tree::tree::SortedLeafIndices;
use crate::patricia_merkle_tree::types::{StarknetForestProofs, StateCommitmentInfos};

/// The information required to write the OS-input commitment infos to the database.
pub struct CommitmentInfosWrite {
    pub block_number: BlockNumber,
    pub keys_digest: [u8; 32],
    pub commitment_infos: StateCommitmentInfos,
}

/// Commitment-infos DB operation, which can be either delete or write.
/// Expected by [ForestWriterWithMetadataAndWitnesses::write_with_metadata_and_commitment_infos],
/// which accumulates all DB operations to guarantee atomicity.
pub enum CommitmentInfosUpdate {
    Write(CommitmentInfosWrite),
    Delete(BlockNumber),
}

/// Reads the committed OS-input commitment infos ([`StateCommitmentInfos`]) for a block height.
#[async_trait]
pub trait ForestReaderWithWitnesses:
    ForestReader<InitialReadContext: EmptyInitialReadContext> + Send
{
    async fn read_commitment_infos(
        &mut self,
        height: BlockNumber,
    ) -> ForestResult<Option<StateCommitmentInfos>>;

    /// Fetches Patricia witness paths for OS input, optionally staging serialized trie node KVs on
    /// an in-memory overlay so reads match post-commit state before the forest is persisted.
    async fn fetch_patricia_witnesses(
        &mut self,
        classes_trie_root_hash: HashOutput,
        contracts_trie_root_hash: HashOutput,
        class_sorted_leaf_indices: SortedLeafIndices<'_>,
        contract_sorted_leaf_indices: SortedLeafIndices<'_>,
        contract_storage_sorted_leaf_indices: &HashMap<NodeIndex, SortedLeafIndices<'_>>,
        staged_serialized_forest: Option<DbHashMap>,
    ) -> TraversalResult<StarknetForestProofs>;
}

/// Writes forest + metadata + deleted nodes, and applies [`CommitmentInfosUpdate`] in the same
/// batch.
#[async_trait]
pub trait ForestWriterWithMetadataAndWitnesses: ForestWriterWithMetadata + Send {
    async fn write_with_metadata_and_commitment_infos(
        &mut self,
        filled_forest: &FilledForest,
        metadata: HashMap<ForestMetadataType, DbValue>,
        deleted_nodes: DeletedNodes,
        commitment_infos_update: CommitmentInfosUpdate,
    ) -> SerializationResult<usize>;
}

/// Forest storage with empty [`ForestReader::InitialReadContext`] plus OS-input witness read/write.
pub trait ForestStorageWithWitnesses:
    ForestReaderWithWitnesses + ForestWriterWithMetadataAndWitnesses + StorageInitializer
{
}

impl<T> ForestStorageWithWitnesses for T where
    T: ForestReaderWithWitnesses + ForestWriterWithMetadataAndWitnesses + StorageInitializer
{
}
