use std::collections::HashMap;
#[cfg(feature = "os_input")]
use std::time::Instant;

#[cfg(feature = "os_input")]
use starknet_api::core::GlobalRoot;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
#[cfg(feature = "os_input")]
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::{NodeIndex, SortedLeafIndices};
use starknet_types_core::felt::Felt;
use tracing::{debug, warn};

use crate::block_committer::errors::BlockCommitmentError;
#[cfg(feature = "os_input")]
use crate::block_committer::errors::CommitBlockWithWitnessesError;
use crate::block_committer::input::{
    contract_address_into_node_index,
    skeleton_storage_updates,
    skeleton_trie_updates,
    Input,
    StarknetStorageValue,
    StateDiff,
};
use crate::block_committer::measurements_util::{
    Action,
    BlockModificationsCounts,
    MeasurementsTrait,
};
#[cfg(feature = "os_input")]
use crate::db::forest_trait::forest_trait_witnesses::ForestStorageWithWitnesses;
use crate::db::forest_trait::ForestReader;
#[cfg(feature = "os_input")]
use crate::db::forest_trait::ForestWriter;
use crate::forest::deleted_nodes::{find_deleted_nodes, DeletedNodes};
use crate::forest::filled_forest::FilledForest;
use crate::forest::forest_errors::ForestError;
use crate::forest::original_skeleton_forest::{ForestSortedIndices, OriginalSkeletonForest};
use crate::forest::updated_skeleton_forest::UpdatedSkeletonForest;
use crate::hash_function::hash::TreeHashFunctionImpl;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
#[cfg(feature = "os_input")]
use crate::patricia_merkle_tree::tree::SortedLeavesRequest;
#[cfg(feature = "os_input")]
use crate::patricia_merkle_tree::types::StateCommitmentInfos;
use crate::patricia_merkle_tree::types::{class_hash_into_node_index, CompiledClassHash};

pub type BlockCommitmentResult<T> = Result<T, BlockCommitmentError>;

pub async fn commit_block<Reader: ForestReader + Send, M: MeasurementsTrait + Send>(
    input: Input<Reader::InitialReadContext>,
    trie_reader: &mut Reader,
    measurements: &mut M,
) -> BlockCommitmentResult<(FilledForest, DeletedNodes)> {
    let mut modified_indices: ForestModifiedIndices = (&input.state_diff).into();
    let n_contracts_trie_modifications = modified_indices.n_contracts_trie_modifications();
    let forest_sorted_indices = modified_indices.as_sorted_indices();

    // Phase 1 - read the original forest from the DB.
    let read_output = commit_block_read_phase(
        &input,
        &forest_sorted_indices,
        n_contracts_trie_modifications,
        trie_reader,
        measurements,
    )
    .await?;

    // Phase 2 - compute the new forest.
    commit_block_compute_phase(
        read_output,
        &input.state_diff.address_to_class_hash,
        &input.state_diff.address_to_nonce,
        measurements,
    )
    .await
}

#[cfg(feature = "os_input")]
pub type CommitBlockWithWitnessesResult<T> = Result<T, CommitBlockWithWitnessesError>;

/// Output of [`commit_block_with_witnesses`].
#[cfg(feature = "os_input")]
pub struct CommitBlockWithWitnessesOutput {
    pub filled_forest: FilledForest,
    pub deleted_nodes: DeletedNodes,
    pub state_commitment_infos: StateCommitmentInfos,
    pub global_root: GlobalRoot,
}

/// Commits a block to the in-memory forest and returns the OS-input Patricia witness paths.
///
/// Phases:
/// 1. Read the original forest skeleton from storage.
/// 2. In parallel: fetch pre-commit witnesses (storage-bound) and compute the updated forest
///    (CPU-bound; touches no storage).
/// 3. Fetch post-commit witnesses against an in-memory overlay of the just-computed forest.
///
/// Does not persist the updated forest — the caller is responsible for writing
/// `filled_forest`/`deleted_nodes` (together with any metadata bundle and the returned
/// `patricia_proofs`) atomically.
#[cfg(feature = "os_input")]
pub async fn commit_block_with_witnesses<Storage, M>(
    input: Input<Storage::InitialReadContext>,
    sorted_leaves: &SortedLeavesRequest<'_>,
    forest_storage: &mut Storage,
    measurements: &mut M,
) -> CommitBlockWithWitnessesResult<CommitBlockWithWitnessesOutput>
where
    Storage: ForestStorageWithWitnesses + Send,
    M: MeasurementsTrait + Send,
{
    let pre_roots = forest_storage
        .read_roots(input.initial_read_context.clone())
        .await
        .map_err(|e| BlockCommitmentError::from(ForestError::from(e)))?;

    let mut modified_indices: ForestModifiedIndices = (&input.state_diff).into();
    let n_contracts_trie_modifications = modified_indices.n_contracts_trie_modifications();
    let forest_sorted_indices = modified_indices.as_sorted_indices();

    // Phase 1 — read the original forest from storage.
    let read_output = commit_block_read_phase(
        &input,
        &forest_sorted_indices,
        n_contracts_trie_modifications,
        forest_storage,
        measurements,
    )
    .await?;

    // Phase 2 — in parallel: fetch pre-commit witnesses while computing the updated forest.
    // The Compute action is recorded by `commit_block_compute_phase`, which holds the &mut
    // borrow of `measurements` for the duration of the join. The fetch branch therefore cannot
    // touch `measurements`; it times itself with a local `Instant`.
    let fetch_witnesses_first_pass = async {
        // TODO(Yoav): Use a util from measurements_util.rs.
        let fetch_timer = Instant::now();
        let fetch_result = forest_storage
            .fetch_patricia_witnesses(
                pre_roots.classes_trie_root_hash,
                pre_roots.contracts_trie_root_hash,
                sorted_leaves.class_sorted,
                sorted_leaves.contract_sorted,
                &sorted_leaves.storage_sorted,
                None,
            )
            .await;
        (fetch_result, fetch_timer.elapsed().as_secs_f64())
    };
    let ((fetch_result, fetch_duration_seconds), compute_result) = tokio::join!(
        fetch_witnesses_first_pass,
        commit_block_compute_phase(
            read_output,
            &input.state_diff.address_to_class_hash,
            &input.state_diff.address_to_nonce,
            measurements,
        ),
    );
    let mut patricia_proofs =
        fetch_result.map_err(CommitBlockWithWitnessesError::PreCommitWitnessFetch)?;
    measurements.record_measurement(
        Action::FetchWitnessesFirstPass,
        patricia_proofs.get_nodes_count(),
        fetch_duration_seconds,
    );
    let (filled_forest, deleted_nodes) = compute_result?;

    // Phase 3 — fetch post-commit witnesses against an in-memory overlay of the new forest.
    let post_roots = filled_forest.state_roots();
    let forest_updates = <Storage as ForestWriter>::serialize_forest(&filled_forest)?;

    measurements.start_measurement(Action::FetchWitnessesSecondPass);
    let proof_after = forest_storage
        .fetch_patricia_witnesses(
            post_roots.classes_trie_root_hash,
            post_roots.contracts_trie_root_hash,
            sorted_leaves.class_sorted,
            sorted_leaves.contract_sorted,
            &sorted_leaves.storage_sorted,
            Some(forest_updates),
        )
        .await
        .map_err(CommitBlockWithWitnessesError::PostCommitWitnessFetch)?;
    measurements
        .attempt_to_stop_measurement(
            Action::FetchWitnessesSecondPass,
            proof_after.get_nodes_count(),
        )
        .ok();

    // Capture each accessed contract's pre-commit storage root before merging the proofs:
    // `extend` overwrites the pre-commit contract leaves with the post-commit ones, after which
    // only the post-commit storage roots remain recoverable from the merged proof.
    let previous_storage_roots: HashMap<ContractAddress, HashOutput> = patricia_proofs
        .contracts_trie_proof
        .leaves
        .iter()
        .map(|(address, contract_state)| (*address, contract_state.storage_root_hash))
        .collect();

    patricia_proofs.extend(proof_after);

    let state_commitment_infos = StateCommitmentInfos::from_commit_witnesses(
        &pre_roots,
        &post_roots,
        &previous_storage_roots,
        &patricia_proofs,
    );

    let global_root = post_roots.global_root();
    Ok(CommitBlockWithWitnessesOutput {
        filled_forest,
        deleted_nodes,
        state_commitment_infos,
        global_root,
    })
}

/// Owns the backing `Vec<NodeIndex>` storage required to build a `ForestSortedIndices`.
/// The owner must outlive the `ForestSortedIndices` borrowed from it.
struct ForestModifiedIndices {
    storage_tries_indices: HashMap<ContractAddress, Vec<NodeIndex>>,
    contracts_trie_indices: Vec<NodeIndex>,
    classes_trie_indices: Vec<NodeIndex>,
}

impl ForestModifiedIndices {
    fn n_contracts_trie_modifications(&self) -> usize {
        self.contracts_trie_indices.len()
    }

    fn as_sorted_indices(&mut self) -> ForestSortedIndices<'_> {
        ForestSortedIndices {
            storage_tries_sorted_indices: self
                .storage_tries_indices
                .iter_mut()
                .map(|(address, indices)| (*address, SortedLeafIndices::new(indices)))
                .collect(),
            contracts_trie_sorted_indices: SortedLeafIndices::new(&mut self.contracts_trie_indices),
            classes_trie_sorted_indices: SortedLeafIndices::new(&mut self.classes_trie_indices),
        }
    }
}

impl From<&StateDiff> for ForestModifiedIndices {
    fn from(state_diff: &StateDiff) -> Self {
        let (storage_tries_indices, contracts_trie_indices, classes_trie_indices) =
            get_all_modified_indices(state_diff);
        Self { storage_tries_indices, contracts_trie_indices, classes_trie_indices }
    }
}

/// Output of the read phase of `commit_block`.
pub struct CommitReadPhaseOutput<'a> {
    pub original_forest: OriginalSkeletonForest<'a>,
    pub original_contracts_trie_leaves: HashMap<NodeIndex, ContractState>,
    pub actual_storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    pub actual_classes_updates: LeafModifications<CompiledClassHash>,
}

/// Read phase of `commit_block`: loads the original forest skeleton from storage.
pub async fn commit_block_read_phase<
    'a,
    Reader: ForestReader + Send,
    M: MeasurementsTrait + Send,
>(
    input: &Input<Reader::InitialReadContext>,
    forest_sorted_indices: &'a ForestSortedIndices<'a>,
    n_contracts_trie_modifications: usize,
    trie_reader: &mut Reader,
    measurements: &mut M,
) -> BlockCommitmentResult<CommitReadPhaseOutput<'a>> {
    let actual_storage_updates = input.state_diff.actual_storage_updates();
    let actual_classes_updates = input.state_diff.actual_classes_updates();
    // Record the number of modifications.
    measure_number_of_modifications(
        measurements,
        &actual_storage_updates,
        n_contracts_trie_modifications,
        actual_classes_updates.len(),
    );

    let (original_forest, original_contracts_trie_leaves) = read_original_forest(
        input,
        &actual_storage_updates,
        &actual_classes_updates,
        forest_sorted_indices,
        trie_reader,
        measurements,
    )
    .await?;

    Ok(CommitReadPhaseOutput {
        original_forest,
        original_contracts_trie_leaves,
        actual_storage_updates,
        actual_classes_updates,
    })
}

/// Reads the original forest from the DB.
async fn read_original_forest<'a, Reader: ForestReader + Send, M: MeasurementsTrait + Send>(
    input: &Input<Reader::InitialReadContext>,
    actual_storage_updates: &HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    actual_classes_updates: &LeafModifications<CompiledClassHash>,
    forest_sorted_indices: &'a ForestSortedIndices<'a>,
    trie_reader: &mut Reader,
    measurements: &mut M,
) -> BlockCommitmentResult<(OriginalSkeletonForest<'a>, HashMap<NodeIndex, ContractState>)> {
    measurements.start_measurement(Action::Read);
    let roots = trie_reader
        .read_roots(input.initial_read_context.clone())
        .await
        .map_err(ForestError::from)?;
    let (original_forest, original_contracts_trie_leaves) = trie_reader
        .read(
            roots,
            actual_storage_updates,
            actual_classes_updates,
            forest_sorted_indices,
            input.config.clone(),
        )
        .await?;
    let n_read_entries = original_forest.storage_tries.values().map(|trie| trie.nodes.len()).sum();
    measurements.attempt_to_stop_measurement(Action::Read, n_read_entries).ok();
    debug!("Original skeleton forest created successfully.");

    if input.config.warn_on_trivial_modifications() {
        check_trivial_nonce_and_class_hash_updates(
            &original_contracts_trie_leaves,
            &input.state_diff.address_to_class_hash,
            &input.state_diff.address_to_nonce,
        );
    }

    Ok((original_forest, original_contracts_trie_leaves))
}

/// Compute phase of `commit_block`: derives the updated forest topology, the new hashes, and
/// the nodes deleted by the state diff. Does not touch storage and may run in parallel with
/// storage-bound work.
pub async fn commit_block_compute_phase<M: MeasurementsTrait + Send>(
    read_output: CommitReadPhaseOutput<'_>,
    address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
    address_to_nonce: &HashMap<ContractAddress, Nonce>,
    measurements: &mut M,
) -> BlockCommitmentResult<(FilledForest, DeletedNodes)> {
    compute_updated_forest(
        read_output.original_forest,
        read_output.original_contracts_trie_leaves,
        read_output.actual_storage_updates,
        read_output.actual_classes_updates,
        address_to_class_hash,
        address_to_nonce,
        measurements,
    )
    .await
}

/// Computes the updated forest topology, its new hashes, and the nodes deleted by the state diff.
async fn compute_updated_forest<M: MeasurementsTrait + Send>(
    original_forest: OriginalSkeletonForest<'_>,
    original_contracts_trie_leaves: HashMap<NodeIndex, ContractState>,
    actual_storage_updates: HashMap<ContractAddress, LeafModifications<StarknetStorageValue>>,
    actual_classes_updates: LeafModifications<CompiledClassHash>,
    address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
    address_to_nonce: &HashMap<ContractAddress, Nonce>,
    measurements: &mut M,
) -> BlockCommitmentResult<(FilledForest, DeletedNodes)> {
    measurements.start_measurement(Action::Compute);

    // Compute the new topology.
    let updated_forest = UpdatedSkeletonForest::create(
        &original_forest,
        &skeleton_trie_updates(&actual_classes_updates),
        &skeleton_storage_updates(&actual_storage_updates),
        &original_contracts_trie_leaves,
        address_to_class_hash,
        address_to_nonce,
    )?;
    debug!("Updated skeleton forest created successfully.");

    // Find deleted nodes.
    let deleted_nodes = find_deleted_nodes(
        &original_forest,
        &updated_forest,
        &actual_storage_updates,
        &actual_classes_updates,
        &original_contracts_trie_leaves,
    );

    // Compute the new hashes.
    let filled_forest = FilledForest::create::<TreeHashFunctionImpl>(
        updated_forest,
        actual_storage_updates,
        actual_classes_updates,
        &original_contracts_trie_leaves,
        address_to_class_hash,
        address_to_nonce,
    )
    .await?;
    measurements.attempt_to_stop_measurement(Action::Compute, 0).ok();
    debug!("Filled forest created successfully.");

    Ok((filled_forest, deleted_nodes))
}

/// Compares the previous state's nonce and class hash with the given in the state diff.
/// In case of trivial update, logs out a warning for trivial state diff update.
fn check_trivial_nonce_and_class_hash_updates(
    original_contracts_trie_leaves: &HashMap<NodeIndex, ContractState>,
    address_to_class_hash: &HashMap<ContractAddress, ClassHash>,
    address_to_nonce: &HashMap<ContractAddress, Nonce>,
) {
    for (address, nonce) in address_to_nonce.iter() {
        if original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .is_some_and(|previous_contract_state| previous_contract_state.nonce == *nonce)
        {
            warn!("Encountered a trivial nonce update of contract {:?}", address)
        }
    }

    for (address, class_hash) in address_to_class_hash.iter() {
        if original_contracts_trie_leaves
            .get(&contract_address_into_node_index(address))
            .is_some_and(|previous_contract_state| {
                previous_contract_state.class_hash == *class_hash
            })
        {
            warn!("Encountered a trivial class hash update of contract {:?}", address)
        }
    }
}

type StorageTriesIndices = HashMap<ContractAddress, Vec<NodeIndex>>;
type ContractsTrieIndices = Vec<NodeIndex>;
type ClassesTrieIndices = Vec<NodeIndex>;

/// Returns all modified indices in the given state diff.
pub(crate) fn get_all_modified_indices(
    state_diff: &StateDiff,
) -> (StorageTriesIndices, ContractsTrieIndices, ClassesTrieIndices) {
    let accessed_addresses = state_diff.accessed_addresses();
    let contracts_trie_indices: Vec<NodeIndex> = accessed_addresses
        .iter()
        .map(|address| contract_address_into_node_index(address))
        .collect();
    let classes_trie_indices: Vec<NodeIndex> = state_diff
        .class_hash_to_compiled_class_hash
        .keys()
        .map(class_hash_into_node_index)
        .collect();
    let storage_tries_indices: HashMap<ContractAddress, Vec<NodeIndex>> = accessed_addresses
        .iter()
        .map(|address| {
            let indices: Vec<NodeIndex> = match state_diff.storage_updates.get(address) {
                Some(updates) => updates.keys().map(NodeIndex::from).collect(),
                None => Vec::new(),
            };
            (**address, indices)
        })
        .collect();
    (storage_tries_indices, contracts_trie_indices, classes_trie_indices)
}

fn measure_number_of_modifications(
    measurements: &mut impl MeasurementsTrait,
    storage_modifications: &HashMap<ContractAddress, HashMap<NodeIndex, StarknetStorageValue>>,
    n_contracts_trie_modifications: usize,
    n_classes_trie_modifications: usize,
) {
    let storage_tries = storage_modifications.values().map(|value| value.len()).sum();
    let emptied_storage_leaves = storage_modifications
        .values()
        .map(|storage_entry| storage_entry.values().filter(|value| value.0 == Felt::ZERO).count())
        .sum::<usize>();
    measurements.set_number_of_modifications(BlockModificationsCounts {
        storage_tries,
        contracts_trie: n_contracts_trie_modifications,
        classes_trie: n_classes_trie_modifications,
        emptied_storage_leaves,
    });
}
