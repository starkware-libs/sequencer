//! Commits state diffs on an in-memory index-layout store through the production witness path,
//! for tests and tools that need OS-input witnesses without a database.

use blockifier::state::accessed_keys::AccessedKeys;
use starknet_api::hash::StateRoots;
use starknet_patricia_storage::map_storage::MapStorage;

use crate::block_committer::commit::{
    commit_block_with_witnesses,
    CommitBlockWithWitnessesOutput,
    CommitBlockWithWitnessesResult,
};
use crate::block_committer::input::{Input, ReaderConfig, StateDiff};
use crate::block_committer::measurements_util::NoMeasurements;
use crate::db::forest_trait::ForestWriter;
use crate::db::index_db::{IndexDb, IndexDbReadContext};
use crate::patricia_merkle_tree::tree::LeavesRequest;
use crate::patricia_merkle_tree::types::StateCommitmentInfos;

/// Commits the state diff, collects the OS-input Patricia witness paths, and returns the new state
/// roots together with the [`StateCommitmentInfos`] needed by the OS.
pub async fn commit_state_diff_with_witnesses(
    index_db: &mut IndexDb<MapStorage>,
    state_diff: StateDiff,
    accessed_keys: &AccessedKeys,
) -> CommitBlockWithWitnessesResult<(StateRoots, StateCommitmentInfos)> {
    let warn_on_trivial_modifications = false;
    let build_storage_tries_concurrently = false; // Map storage is used, no need for concurrency.
    let config = ReaderConfig::new(warn_on_trivial_modifications, build_storage_tries_concurrently);
    let input = Input { state_diff, initial_read_context: IndexDbReadContext, config };

    let mut leaves_request = LeavesRequest::from(accessed_keys);
    let sorted_leaves = leaves_request.sorted();

    let CommitBlockWithWitnessesOutput {
        filled_forest,
        deleted_nodes: _,
        state_commitment_infos,
        global_root: _,
    } = commit_block_with_witnesses(input, &sorted_leaves, index_db, &mut NoMeasurements).await?;
    index_db.write(&filled_forest).await?;

    let state_roots = StateRoots {
        contracts_trie_root_hash: filled_forest.get_contract_root_hash(),
        classes_trie_root_hash: filled_forest.get_compiled_class_root_hash(),
    };
    Ok((state_roots, state_commitment_infos))
}

/// Commits the state diff (without collecting witnesses), persists the new forest, and returns the
/// new state roots.
pub async fn commit_state_diff_to_index_db(
    index_db: &mut IndexDb<MapStorage>,
    state_diff: StateDiff,
) -> CommitBlockWithWitnessesResult<StateRoots> {
    let (state_roots, _state_commitment_infos) =
        commit_state_diff_with_witnesses(index_db, state_diff, &AccessedKeys::default()).await?;
    Ok(state_roots)
}
