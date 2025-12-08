use std::collections::HashMap;
use std::future::Future;
use std::sync::LazyLock;

use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, StateDiffCommitment};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::storage_trait::{create_db_key, DbKey, DbKeyPrefix, DbValue};

use crate::block_committer::input::{ConfigImpl, StarknetStorageValue};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestResult;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::CompiledClassHash;

pub static COMMITMENT_OFFSET_KEY: LazyLock<&[u8]> = LazyLock::new(|| b"commitment_offset");
pub static STATE_DIFF_HASH_PREFIX: LazyLock<&[u8]> = LazyLock::new(|| b"state_diff_hash");

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

pub trait ForestWriter {
    /// Returns the number of new facts written to storage.
    fn write(&mut self, filled_forest: &FilledForest) -> impl Future<Output = usize> + Send;
}

pub trait ForestStorage<'a>: ForestReader<'a> + ForestWriter {}

pub(crate) fn commitment_offset_entry(block_height: BlockNumber) -> (DbKey, DbValue) {
    (DbKey(COMMITMENT_OFFSET_KEY.to_vec()), DbValue(block_height.0.to_be_bytes().to_vec()))
}

pub(crate) fn state_diff_hash_entry(
    block_height: BlockNumber,
    state_diff_hash: StateDiffCommitment,
) -> (DbKey, DbValue) {
    let state_diff_hash_key_prefix = DbKeyPrefix::new(*STATE_DIFF_HASH_PREFIX);
    let state_diff_hash_key =
        create_db_key(state_diff_hash_key_prefix, &block_height.0.to_be_bytes());
    let state_diff_hash_bytes = state_diff_hash.0.0.to_bytes_be().to_vec();
    (state_diff_hash_key, DbValue(state_diff_hash_bytes))
}
