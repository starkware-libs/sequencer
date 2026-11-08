use std::collections::HashMap;

use apollo_consensus_orchestrator::cende::BlockAccessedKeysData;
use blockifier::state::accessed_keys::AccessedKeys;
use blockifier::state::cached_state::{CommitmentStateDiff, StateMaps};
use serde::{Deserialize, Serialize};
use starknet_api::hash::HashOutput;
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::commit::CommitBlockWithWitnessesResult;
use starknet_committer::block_committer::in_memory_commit::{
    commit_state_diff_to_index_db,
    commit_state_diff_with_witnesses,
};
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_committer::db::index_db::IndexDb;
use starknet_committer::patricia_merkle_tree::types::CompressedStateCommitmentInfos;
use starknet_patricia_storage::map_storage::MapStorage;
use tracing::info;

use crate::shared_utils::read::{load_input, write_to_file};

#[cfg(test)]
#[path = "commit_with_witnesses_test.rs"]
mod commit_with_witnesses_test;

#[derive(Debug, Deserialize)]
pub struct CommitWithWitnessesInput {
    /// The state diffs of the blocks preceding the block, from the empty state, in block order.
    pub committed_state_diffs: Vec<ThinStateDiff>,
    /// The state diff of the block.
    pub state_diff: ThinStateDiff,
    /// The block's execution data in the format of the cende recorder's `get_accessed_keys_input`
    /// response.
    pub accessed_keys_input: BlockAccessedKeysData,
}

/// Serialized in the cende blob's encodings: `state_commitment_infos` as the blob's
/// `recent_state_commitment_infos` entries carry it, `initial_reads` as the blob's field.
#[derive(Debug, Serialize)]
pub struct CommitWithWitnessesOutput {
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
    pub state_commitment_infos: CompressedStateCommitmentInfos,
    pub initial_reads: StateMaps,
}

pub async fn parse_and_commit_with_witnesses(input_path: String, output_path: String) {
    let input = load_input::<CommitWithWitnessesInput>(input_path);
    let output = commit_with_witnesses(input).await.expect("Failed to commit the given block.");
    write_to_file(&output_path, &output);
    info!(
        "Successfully committed the given block. Updated Contracts Trie Root Hash: {:?}, Updated \
         Classes Trie Root Hash: {:?}",
        output.contracts_trie_root_hash, output.classes_trie_root_hash,
    );
}

pub async fn commit_with_witnesses(
    input: CommitWithWitnessesInput,
) -> CommitBlockWithWitnessesResult<CommitWithWitnessesOutput> {
    let CommitWithWitnessesInput { committed_state_diffs, state_diff, accessed_keys_input } = input;
    let mut index_db = IndexDb::new(MapStorage::default());
    let mut state = StateMaps::default();
    for committed_state_diff in committed_state_diffs {
        commit_state_diff_to_index_db(&mut index_db, committed_state_diff.clone().into()).await?;
        state.extend(&StateMaps::from(CommitmentStateDiff::from(committed_state_diff)));
    }

    let accessed_keys =
        accessed_keys_input.compute_accessed_keys(&CommitmentStateDiff::from(state_diff.clone()));
    let initial_reads = pre_block_values(&state, &accessed_keys);
    let (state_roots, state_commitment_infos) =
        commit_state_diff_with_witnesses(&mut index_db, state_diff.into(), &accessed_keys).await?;
    let state_commitment_infos =
        state_commitment_infos.compress().expect("Failed to compress the state commitment infos.");

    Ok(CommitWithWitnessesOutput {
        contracts_trie_root_hash: state_roots.contracts_trie_root_hash,
        classes_trie_root_hash: state_roots.classes_trie_root_hash,
        state_commitment_infos,
        initial_reads,
    })
}

/// The pre-block value of every accessed key, zero when the key was never written. A superset of
/// the reads the batcher records: it carries every key the witnesses cover.
fn pre_block_values(state: &StateMaps, accessed_keys: &AccessedKeys) -> StateMaps {
    fn lookup<K: Copy + Eq + std::hash::Hash, V: Copy + Default>(
        map: &HashMap<K, V>,
        keys: impl IntoIterator<Item = K>,
    ) -> HashMap<K, V> {
        keys.into_iter().map(|key| (key, map.get(&key).copied().unwrap_or_default())).collect()
    }
    StateMaps {
        nonces: lookup(&state.nonces, accessed_keys.accessed_contracts.iter().copied()),
        class_hashes: lookup(&state.class_hashes, accessed_keys.accessed_contracts.iter().copied()),
        storage: lookup(&state.storage, accessed_keys.storage_keys.iter().copied()),
        compiled_class_hashes: lookup(
            &state.compiled_class_hashes,
            accessed_keys.accessed_class_hashes.iter().copied(),
        ),
        // The OS ignores it; empty in the blob's initial reads as well.
        declared_contracts: HashMap::new(),
    }
}
