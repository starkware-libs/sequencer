use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use apollo_committer_config::config::{ApolloStorage, CommitterConfig};
use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
#[cfg(feature = "os_input")]
use apollo_committer_types::committer_types::{
    ReadPathsAndCommitBlockRequest,
    ReadPathsAndCommitBlockResponse,
};
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::hash::PoseidonHash;
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::commit::commit_block;
#[cfg(feature = "os_input")]
use starknet_committer::block_committer::commit::{
    commit_block_with_witnesses,
    CommitBlockWithWitnessesOutput,
};
use starknet_committer::block_committer::input::Input;
use starknet_committer::block_committer::measurements_util::{
    Action,
    BlockDurations,
    BlockMeasurement,
    BlockModificationsCounts,
    MeasurementsTrait,
    SingleBlockMeasurements,
};
#[cfg(feature = "os_input")]
use starknet_committer::db::forest_trait::forest_trait_witnesses::{
    CommitmentInfosUpdate,
    CommitmentInfosWrite,
    ForestStorageWithWitnesses,
};
use starknet_committer::db::forest_trait::{
    EmptyInitialReadContext,
    ForestMetadataType,
    ForestStorageWithEmptyReadContext,
};
use starknet_committer::db::index_db::IndexDb;
#[cfg(feature = "os_input")]
use starknet_committer::db::serde_db_utils::accessed_keys_digest;
use starknet_committer::db::serde_db_utils::{
    deserialize_felt_no_packing,
    serialize_felt_no_packing,
    DbBlockNumber,
};
use starknet_committer::forest::deleted_nodes::DeletedNodes;
use starknet_committer::forest::filled_forest::FilledForest;
#[cfg(feature = "os_input")]
use starknet_committer::patricia_merkle_tree::tree::LeavesRequest;
#[cfg(feature = "os_input")]
use starknet_committer::patricia_merkle_tree::types::StateCommitmentInfos;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::errors::SerializationError;
use starknet_patricia_storage::map_storage::CachedStorage;
use starknet_patricia_storage::rocksdb_storage::RocksDbStorage;
#[cfg(feature = "os_input")]
use starknet_patricia_storage::storage_trait::ImmutableReadOnlyStorage;
use starknet_patricia_storage::storage_trait::{DbValue, Storage};
use tracing::{debug, error, info, warn};

use crate::metrics::{
    register_metrics,
    AVERAGE_COMPUTE_RATE,
    AVERAGE_READ_RATE,
    AVERAGE_WRITE_RATE,
    BLOCKS_COMMITTED,
    COMMITTER_OFFSET,
    COMPUTE_DURATION_PER_BLOCK,
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK,
    COUNT_EMPTIED_LEAVES_PER_BLOCK,
    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK,
    READ_DURATION_PER_BLOCK,
    TOTAL_BLOCK_DURATION,
    TOTAL_BLOCK_DURATION_PER_MODIFICATION,
    WRITE_DURATION_PER_BLOCK,
};

#[cfg(test)]
#[path = "committer_test.rs"]
mod committer_test;

pub type ApolloCommitterDb = IndexDb<ApolloStorage>;

pub type ApolloCommitter = Committer<ApolloStorage, ApolloCommitterDb>;

pub trait StorageConstructor: Storage {
    fn create_storage(db_path: PathBuf, storage_config: Self::Config) -> Self;
}

impl StorageConstructor for ApolloStorage {
    fn create_storage(db_path: PathBuf, storage_config: Self::Config) -> Self {
        let rocksdb_storage =
            RocksDbStorage::new(&db_path, storage_config.inner_storage_config.clone())
                .inspect_err(|e| error!("Failed to open committer DB: {e}"))
                .unwrap();
        CachedStorage::new(rocksdb_storage, storage_config)
    }
}

struct CommitStateDiffOutput {
    pub filled_forest: FilledForest,
    pub deleted_nodes: DeletedNodes,
    pub global_root: GlobalRoot,
}

/// Classification of a commit request's `height` w.r.t the committer offset.
enum CommitBlockHeightPlan {
    /// `height` is already committed; return the stored global root without writing.
    Historical { global_root: GlobalRoot },
    /// `height` is the next uncommitted offset; inputs are validated and ready to commit.
    CommitTip { state_diff_commitment: StateDiffCommitment },
}

fn commit_tip_metadata_bundle(
    height: BlockNumber,
    global_root: GlobalRoot,
    state_diff_commitment: StateDiffCommitment,
) -> (HashMap<ForestMetadataType, DbValue>, BlockNumber) {
    let next_offset = height.unchecked_next();
    (
        HashMap::from([
            (
                ForestMetadataType::CommitmentOffset,
                DbValue(DbBlockNumber(next_offset).serialize().to_vec()),
            ),
            (
                ForestMetadataType::StateRoot(DbBlockNumber(height)),
                serialize_felt_no_packing(global_root.0),
            ),
            (
                ForestMetadataType::StateDiffHash(DbBlockNumber(height)),
                serialize_felt_no_packing(state_diff_commitment.0.0),
            ),
        ]),
        next_offset,
    )
}

/// Compresses the state commitment infos to `base64(zstd(serde_json(..)))`, once at the source, so
/// the witness stays compact across the committer->batcher RPC (which has an 8 MB response cap),
/// batcher storage, and the cende blob.
#[cfg(feature = "os_input")]
fn compress_state_commitment_infos(infos: &StateCommitmentInfos) -> CommitterResult<String> {
    let json = serde_json::to_vec(infos)
        .map_err(|err| CommitterError::StateCommitmentInfosSerialization(err.to_string()))?;
    let compressed = zstd::encode_all(json.as_slice(), STATE_COMMITMENT_INFOS_ZSTD_LEVEL)
        .map_err(|err| CommitterError::StateCommitmentInfosCompression(err.to_string()))?;
    Ok(base64::encode(compressed))
}

#[cfg(feature = "os_input")]
const STATE_COMMITMENT_INFOS_ZSTD_LEVEL: i32 = 3;

/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: Storage, ForestDB>
where
    ForestDB: ForestStorageWithEmptyReadContext,
{
    /// Storage for forest operations.
    forest_storage: ForestDB,
    /// Committer config.
    config: CommitterConfig<S::Config>,
    /// The next block number to commit.
    offset: BlockNumber,
}

impl<S, ForestDB> Committer<S, ForestDB>
where
    S: StorageConstructor,
    ForestDB: ForestStorageWithEmptyReadContext<Storage = S>,
{
    pub async fn new(config: CommitterConfig<S::Config>) -> Self {
        let storage = S::create_storage(config.db_path.clone(), config.storage_config.clone());
        let mut forest_storage = ForestDB::new(storage);
        let offset = Self::load_offset_or_panic(&mut forest_storage).await;
        info!("Initializing committer with offset: {offset}");
        Self { forest_storage, config, offset }
    }

    fn update_offset(&mut self, offset: BlockNumber) {
        self.offset = offset;
        COMMITTER_OFFSET.set_lossy(offset.0);
    }

    /// Commits a block to the forest.
    /// In the happy flow, the given height equals to the committer offset.
    pub async fn commit_block(
        &mut self,
        CommitBlockRequest { state_diff, state_diff_commitment, height }: CommitBlockRequest,
    ) -> CommitterResult<CommitBlockResponse> {
        let result = self
            .commit_block_inner(CommitBlockRequest { state_diff, state_diff_commitment, height })
            .await;
        match &result {
            Ok(_) => {
                debug!("Committed block number {height} with state diff {state_diff_commitment:?}");
            }
            Err(err) => {
                error!("Failed to commit block number {height}: {err:?}");
            }
        };
        result
    }

    /// Commits a block to the forest.
    /// In the happy flow, the given height equals to the committer offset.
    async fn commit_block_inner(
        &mut self,
        CommitBlockRequest { state_diff, state_diff_commitment, height }: CommitBlockRequest,
    ) -> CommitterResult<CommitBlockResponse> {
        info!(
            "Received request to commit block number {height} with state diff \
             {state_diff_commitment:?}"
        );

        match self.commit_or_load(&state_diff, state_diff_commitment, height).await? {
            CommitBlockHeightPlan::Historical { global_root } => {
                Ok(CommitBlockResponse { global_root })
            }
            CommitBlockHeightPlan::CommitTip { state_diff_commitment } => {
                // Happy flow. Commits the state diff and returns the computed global root.
                debug!(
                    "Committing block number {height} with state diff {state_diff_commitment:?}"
                );
                let mut block_measurements = SingleBlockMeasurements::default();
                block_measurements.start_measurement(Action::EndToEnd);
                let CommitStateDiffOutput { filled_forest, global_root, deleted_nodes } =
                    self.commit_state_diff(state_diff, &mut block_measurements).await?;
                let (metadata, next_offset) =
                    commit_tip_metadata_bundle(height, global_root, state_diff_commitment);
                info!(
                    "For block number {height}, writing filled forest to storage with metadata: \
                     {metadata:?}, delete {} nodes",
                    deleted_nodes.len()
                );
                block_measurements.start_measurement(Action::Write);
                let n_write_entries = self
                    .forest_storage
                    .write_with_metadata(&filled_forest, metadata, deleted_nodes)
                    .await
                    .map_err(|err| self.map_internal_error(err))?;
                block_measurements.attempt_to_stop_measurement(Action::Write, n_write_entries).ok();
                block_measurements.attempt_to_stop_measurement(Action::EndToEnd, 0).ok();
                update_metrics(height, &block_measurements.block_measurement);
                self.update_offset(next_offset);
                Ok(CommitBlockResponse { global_root })
            }
        }
    }

    /// Either load the committed global root for a past `height`, or validate inputs and return
    /// the state-diff commitment for committing the chain tip at `self.offset`.
    async fn commit_or_load(
        &mut self,
        state_diff: &ThinStateDiff,
        state_diff_commitment: Option<StateDiffCommitment>,
        height: BlockNumber,
    ) -> CommitterResult<CommitBlockHeightPlan> {
        if height > self.offset {
            // Request to commit a future height.
            // Returns an error, indicating the committer has a hole in the state diff series.
            return Err(CommitterError::CommitHeightHole {
                input_height: height,
                committer_offset: self.offset,
            });
        }

        let state_diff_commitment = match state_diff_commitment {
            Some(commitment) => {
                if self.config.verify_state_diff_hash {
                    let calculated_commitment = calculate_state_diff_hash(state_diff);
                    if commitment != calculated_commitment {
                        return Err(CommitterError::StateDiffHashMismatch {
                            provided_commitment: commitment,
                            calculated_commitment,
                            height,
                        });
                    }
                }
                commitment
            }
            None => calculate_state_diff_hash(state_diff),
        };

        if height < self.offset {
            // Request to commit an old height.
            // Might be ok if the caller didn't get the results properly.
            warn!(
                "Received request to commit an old block number {height}. The committer offset is \
                 {0}.",
                self.offset
            );
            let stored_state_diff_commitment = self.load_state_diff_commitment(height).await?;
            // Verify the input state diff matches the stored one by comparing the commitments.
            if state_diff_commitment != stored_state_diff_commitment {
                return Err(CommitterError::InvalidStateDiffCommitment {
                    input_commitment: state_diff_commitment,
                    stored_commitment: stored_state_diff_commitment,
                    height,
                });
            }
            // Returns the precomputed global root.
            let db_global_root = self.load_global_root(height).await?;
            return Ok(CommitBlockHeightPlan::Historical { global_root: db_global_root });
        }

        Ok(CommitBlockHeightPlan::CommitTip { state_diff_commitment })
    }

    /// Applies the given state diff to revert the changes of the given height.
    pub async fn revert_block(
        &mut self,
        RevertBlockRequest { reversed_state_diff, height }: RevertBlockRequest,
    ) -> CommitterResult<RevertBlockResponse> {
        let result =
            self.revert_block_inner(RevertBlockRequest { reversed_state_diff, height }).await;
        match &result {
            Ok(_) => {
                info!("Reverted block number {height}");
            }
            Err(err) => {
                error!("Failed to revert block number {height}: {err:?}");
            }
        };
        result
    }

    /// Applies the given state diff to revert the changes of the given height.
    async fn revert_block_inner(
        &mut self,
        RevertBlockRequest { reversed_state_diff, height }: RevertBlockRequest,
    ) -> CommitterResult<RevertBlockResponse> {
        info!("Received request to revert block number {height}");
        let Some(last_committed_block) = self.offset.prev() else {
            // No committed blocks. Nothing to revert.
            warn!("Received request to revert block number {height}. No committed blocks.");
            return Ok(RevertBlockResponse::Uncommitted);
        };

        if height > self.offset {
            // Request to revert a future height. Nothing to revert.
            warn!(
                "Received request to revert a future block number {height}. The committer offset \
                 is {0}.",
                self.offset
            );
            return Ok(RevertBlockResponse::Uncommitted);
        }

        if height == self.offset {
            // Request to revert the next future height.
            // Nothing to revert, but we have the resulted state root.
            warn!("Received request to revert the committer offset height block {height}.");
            let db_state_root = self.load_global_root(last_committed_block).await?;
            return Ok(RevertBlockResponse::AlreadyReverted(db_state_root));
        }

        if height < last_committed_block {
            // Request to revert an old height. Nothing to revert.
            // Returns an error, indicating the committer has a hole in the revert series.
            return Err(CommitterError::RevertHeightHole {
                input_height: height,
                last_committed_block,
            });
        }

        debug!("Reverting block number {height}");
        // Sanity.
        assert_eq!(height, last_committed_block);
        // Happy flow. Reverts the state diff and returns the computed global root.
        let mut block_measurements = SingleBlockMeasurements::default();
        block_measurements.start_measurement(Action::EndToEnd);
        let CommitStateDiffOutput { filled_forest, global_root: revert_global_root, deleted_nodes } =
            self.commit_state_diff(reversed_state_diff, &mut block_measurements).await?;

        // The last committed block is offset-1. After the revert, the last committed block wll be
        // offset-2 (if exists).
        if let Some(prev_committed_block) = last_committed_block.prev() {
            // In revert flow, in contrast to commit flow, we can't verify that the reversed state
            // diff matches the state diff commitment in storage. However, we can verify that the
            // post-revert global root matches the stored global root of the one before the last
            // committed block.
            let stored_global_root = self.load_global_root(prev_committed_block).await?;
            if stored_global_root != revert_global_root {
                // The revert global root is not the same as the stored global root.
                return Err(CommitterError::InvalidRevertedGlobalRoot {
                    input_global_root: revert_global_root,
                    stored_global_root,
                    height: prev_committed_block,
                });
            }
        }

        // Ignore entries with block number key equals to or higher than the offset.
        let metadata = HashMap::from([(
            ForestMetadataType::CommitmentOffset,
            DbValue(DbBlockNumber(last_committed_block).serialize().to_vec()),
        )]);
        info!(
            "For block number {height}, writing filled forest and updating the commitment offset \
             to {last_committed_block}"
        );
        block_measurements.start_measurement(Action::Write);
        let n_write_entries = {
            #[cfg(not(feature = "os_input"))]
            {
                self.forest_storage
                    .write_with_metadata(&filled_forest, metadata, deleted_nodes)
                    .await
            }
            #[cfg(feature = "os_input")]
            {
                self.forest_storage
                    .write_with_metadata_and_commitment_infos(
                        &filled_forest,
                        metadata,
                        deleted_nodes,
                        CommitmentInfosUpdate::Delete(height),
                    )
                    .await
            }
        }
        .map_err(|err| self.map_internal_error(err))?;
        block_measurements.attempt_to_stop_measurement(Action::Write, n_write_entries).ok();
        block_measurements.attempt_to_stop_measurement(Action::EndToEnd, 0).ok();
        update_metrics(height, &block_measurements.block_measurement);
        self.update_offset(last_committed_block);
        Ok(RevertBlockResponse::RevertedTo(revert_global_root))
    }

    async fn load_state_diff_commitment(
        &mut self,
        block_number: BlockNumber,
    ) -> CommitterResult<StateDiffCommitment> {
        let db_value = self
            .read_metadata(ForestMetadataType::StateDiffHash(DbBlockNumber(block_number)))
            .await?;
        Ok(StateDiffCommitment(PoseidonHash(deserialize_felt_no_packing(&db_value))))
    }

    async fn load_global_root(&mut self, block_number: BlockNumber) -> CommitterResult<GlobalRoot> {
        let db_value =
            self.read_metadata(ForestMetadataType::StateRoot(DbBlockNumber(block_number))).await?;
        Ok(GlobalRoot(deserialize_felt_no_packing(&db_value)))
    }

    async fn load_offset_or_panic(forest_storage: &mut ForestDB) -> BlockNumber {
        let db_offset = forest_storage
            .read_metadata(ForestMetadataType::CommitmentOffset)
            .await
            .expect("Failed to read commitment offset");

        db_offset
            .map(|value| {
                let array_value: [u8; 8] = value.0.try_into().unwrap_or_else(|value| {
                    panic!("Failed to deserialize commitment offset from {value:?}")
                });
                DbBlockNumber::deserialize(array_value)
            })
            .unwrap_or_default()
            .0
    }

    // Reads metadata from the storage, returns an error if it is not found.
    async fn read_metadata(&mut self, metadata: ForestMetadataType) -> CommitterResult<DbValue> {
        self.forest_storage
            .read_metadata(metadata.clone())
            .await
            .map_err(|err| self.map_internal_error(err))?
            .ok_or(CommitterError::MissingMetadata(metadata))
    }

    async fn commit_state_diff<M: MeasurementsTrait + Send>(
        &mut self,
        state_diff: ThinStateDiff,
        measurements: &mut M,
    ) -> CommitterResult<CommitStateDiffOutput> {
        let input = Input {
            state_diff: state_diff.into(),
            initial_read_context: ForestDB::InitialReadContext::create_empty(),
            config: self.config.reader_config.clone(),
        };
        let (filled_forest, deleted_nodes) =
            commit_block(input, &mut self.forest_storage, measurements)
                .await
                .map_err(|err| self.map_internal_error(err))?;
        let global_root = filled_forest.state_roots().global_root();
        Ok(CommitStateDiffOutput { filled_forest, global_root, deleted_nodes })
    }

    fn map_internal_error<E: Error>(&self, err: E) -> CommitterError {
        self.map_internal_error_at_height(self.offset, err)
    }

    fn map_internal_error_at_height<E: Error>(
        &self,
        height: BlockNumber,
        err: E,
    ) -> CommitterError {
        let error_message = format!("{err:?}: {err}");
        error!("Error committing block number {height}. {error_message}.");
        CommitterError::Internal { height, message: error_message }
    }
}

#[cfg(feature = "os_input")]
impl<S, ForestDB> Committer<S, ForestDB>
where
    S: StorageConstructor + ImmutableReadOnlyStorage + 'static,
    ForestDB: ForestStorageWithWitnesses<Storage = S>,
{
    /// Commits the next block and returns merged Patricia witness facts for OS input, persisting
    /// digest + payload for idempotent replay.
    pub async fn read_paths_and_commit_block(
        &mut self,
        ReadPathsAndCommitBlockRequest {
            commit: CommitBlockRequest { state_diff, state_diff_commitment, height },
            accessed_keys,
        }: ReadPathsAndCommitBlockRequest,
    ) -> CommitterResult<ReadPathsAndCommitBlockResponse> {
        let mut leaves_request = LeavesRequest::from(&accessed_keys);
        info!(
            "read_paths_and_commit_block: height {height}, accessed keys len {}, state diff len {}",
            leaves_request.total_leaf_count(),
            state_diff.len(),
        );
        let sorted_leaves = leaves_request.sorted();
        let digest = accessed_keys_digest(&sorted_leaves);

        match self.commit_or_load(&state_diff, state_diff_commitment, height).await? {
            CommitBlockHeightPlan::Historical { global_root } => {
                let stored_digest = self.load_witnesses_digest(height).await?;
                if stored_digest != Some(digest) {
                    return Err(CommitterError::AccessedKeysDigestMismatch {
                        height,
                        stored: stored_digest,
                        expected: digest,
                    });
                }
                let state_commitment_infos = self
                    .forest_storage
                    .read_commitment_infos(height)
                    .await
                    .map_err(|error| self.map_internal_error_at_height(height, error))?
                    .ok_or(CommitterError::MissingPatriciaPaths { height })?;
                Ok(ReadPathsAndCommitBlockResponse {
                    global_root,
                    state_commitment_infos: compress_state_commitment_infos(
                        &state_commitment_infos,
                    )?,
                })
            }
            // Flow overview:
            // 1. Fetch patricia paths for the accessed keys.
            // 2. Compute the updates from the state diff (commit) but avoid updating the underlying
            //    DB in order to guarantee atomicity.
            // 3. Fetch patricia paths for the post-commit tries, via running step 1 against a two
            //    layer storage composed from the underlying storage and the modifications from 2.
            // 4. Merge the two sets of patricia paths and write the result to the storage.
            // 5. Update the commitment offset and return the global root and the patricia proofs.
            CommitBlockHeightPlan::CommitTip { state_diff_commitment } => {
                let mut block_measurements = SingleBlockMeasurements::default();
                block_measurements.start_measurement(Action::EndToEnd);

                let input = Input {
                    state_diff: state_diff.into(),
                    initial_read_context: ForestDB::InitialReadContext::create_empty(),
                    config: self.config.reader_config.clone(),
                };

                let CommitBlockWithWitnessesOutput {
                    filled_forest,
                    deleted_nodes,
                    state_commitment_infos,
                    global_root,
                } = commit_block_with_witnesses(
                    input,
                    &sorted_leaves,
                    &mut self.forest_storage,
                    &mut block_measurements,
                )
                .await
                .map_err(|err| self.map_internal_error(err))?;

                let (metadata, next_offset) =
                    commit_tip_metadata_bundle(height, global_root, state_diff_commitment);

                info!(
                    "For block number {height}, writing filled forest and \
                     {commitment_facts_count} commitment facts to storage with metadata: \
                     {metadata:?}, delete {deleted_nodes_count} nodes",
                    commitment_facts_count = state_commitment_infos.n_commitment_facts(),
                    deleted_nodes_count = deleted_nodes.len(),
                );
                block_measurements.start_measurement(Action::Write);
                let n_write_entries = self
                    .forest_storage
                    .write_with_metadata_and_commitment_infos(
                        &filled_forest,
                        metadata,
                        deleted_nodes,
                        CommitmentInfosUpdate::Write(CommitmentInfosWrite {
                            block_number: height,
                            keys_digest: digest,
                            commitment_infos: state_commitment_infos.clone(),
                        }),
                    )
                    .await
                    .map_err(|e: SerializationError| self.map_internal_error(e))?;
                block_measurements.attempt_to_stop_measurement(Action::Write, n_write_entries).ok();
                block_measurements.attempt_to_stop_measurement(Action::EndToEnd, 0).ok();
                update_metrics(height, &block_measurements.block_measurement);
                self.update_offset(next_offset);
                Ok(ReadPathsAndCommitBlockResponse {
                    global_root,
                    state_commitment_infos: compress_state_commitment_infos(
                        &state_commitment_infos,
                    )?,
                })
            }
        }
    }

    async fn load_witnesses_digest(
        &mut self,
        block_number: BlockNumber,
    ) -> CommitterResult<Option<[u8; 32]>> {
        let digest_raw = self
            .forest_storage
            .read_metadata(ForestMetadataType::AccessedKeysDigest(DbBlockNumber(block_number)))
            .await
            .map_err(|error| self.map_internal_error_at_height(block_number, error))?;

        digest_raw
            .map(|digest_raw| {
                digest_raw.0.as_slice().try_into().map_err(|_| CommitterError::Internal {
                    height: block_number,
                    message: format!(
                        "Invalid OS witnesses digest length {} (expected 32)",
                        digest_raw.0.len()
                    ),
                })
            })
            .transpose()
    }
}

#[async_trait]
impl ComponentStarter for ApolloCommitter {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics(self.offset);
    }
}

#[allow(clippy::as_conversions)]
// TODO(Ariel): Consider adding fetch witnesses measurements.
fn update_metrics(
    height: BlockNumber,
    BlockMeasurement {
        n_reads,
        n_writes,
        durations,
        modifications_counts,
        #[cfg(feature = "os_input")]
        fetched_witnesses_count,
        // TODO(Yoav): Remove the ".." where os_input becomes default.
        // It is needed now for including `BlockMeasurement::fetched_witnesses_count` where
        // `starknet_committer/os_input` is enabled by other crates, while
        // `apollo_committer/os_input` is disabled.
        ..
    }: &BlockMeasurement,
) {
    BLOCKS_COMMITTED.increment(1);
    TOTAL_BLOCK_DURATION.increment((durations.block * 1000.0) as u64);
    let n_modifications = modifications_counts.total();
    // Microseconds.
    let total_block_duration_per_modification = if n_modifications > 0 {
        let total_block_duration_per_modification =
            durations.block / n_modifications as f64 * 1_000_000.0;
        TOTAL_BLOCK_DURATION_PER_MODIFICATION
            .increment(total_block_duration_per_modification as u64);
        Some(total_block_duration_per_modification)
    } else {
        None
    };
    READ_DURATION_PER_BLOCK.increment((durations.read * 1000.0) as u64);
    COMPUTE_DURATION_PER_BLOCK.increment((durations.compute * 1000.0) as u64);
    WRITE_DURATION_PER_BLOCK.increment((durations.write * 1000.0) as u64);

    let read_rate = if durations.read > 0.0 {
        let rate = *n_reads as f64 / durations.read;
        AVERAGE_READ_RATE.increment(rate as u64);
        Some(rate)
    } else {
        None
    };
    let compute_rate = if durations.compute > 0.0 {
        let rate = *n_writes as f64 / durations.compute;
        AVERAGE_COMPUTE_RATE.increment(rate as u64);
        Some(rate)
    } else {
        None
    };
    let write_rate = if durations.write > 0.0 {
        let rate = *n_writes as f64 / durations.write;
        AVERAGE_WRITE_RATE.increment(rate as u64);
        Some(rate)
    } else {
        None
    };

    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK
        .increment(modifications_counts.storage_tries as u64);
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK
        .increment(modifications_counts.contracts_trie as u64);
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK.increment(modifications_counts.classes_trie as u64);
    COUNT_EMPTIED_LEAVES_PER_BLOCK.increment(modifications_counts.emptied_storage_leaves as u64);

    let emptied_leaves_percentage = if modifications_counts.storage_tries > 0 {
        let percentage = modifications_counts.emptied_storage_leaves as f64
            / modifications_counts.storage_tries as f64;
        Some(percentage * 100.0)
    } else {
        None
    };

    log_block_measurements(
        height,
        durations,
        total_block_duration_per_modification,
        read_rate,
        compute_rate,
        write_rate,
        modifications_counts,
        emptied_leaves_percentage,
        #[cfg(feature = "os_input")]
        *fetched_witnesses_count,
    );
}

#[allow(clippy::too_many_arguments)]
fn log_block_measurements(
    height: BlockNumber,
    durations: &BlockDurations,
    total_block_duration_per_modification: Option<f64>,
    read_rate: Option<f64>,
    compute_rate: Option<f64>,
    write_rate: Option<f64>,
    modifications_counts: &BlockModificationsCounts,
    emptied_leaves_percentage: Option<f64>,
    #[cfg(feature = "os_input")] fetched_witnesses_count: usize,
) {
    #[cfg(feature = "os_input")]
    let witness_log = format!(
        "witness fetch ms (pre-commit/post-commit): {:.0}/{:.0}, witness entries: {}",
        durations.fetch_witnesses_first_pass * 1000.0,
        durations.fetch_witnesses_second_pass * 1000.0,
        fetched_witnesses_count,
    );
    #[cfg(not(feature = "os_input"))]
    let witness_log = String::new();

    debug!(
        "Block {height} stats: durations in ms (total/read/compute/write): \
         {:.0}/{:.0}/{:.0}/{:.0}, total block duration per modification in µs: {}, rates in \
         entries/sec (read/compute/write): {}/{}/{}, modifications count \
         (storage_tries/contracts_trie/classes_trie/emptied_storage_leaves): {}/{}/{}/{}{}, \
         {witness_log}",
        durations.block * 1000.0,
        durations.read * 1000.0,
        durations.compute * 1000.0,
        durations.write * 1000.0,
        total_block_duration_per_modification.map_or(String::new(), |d| format!("{d:.0}µs")),
        read_rate.map_or(String::new(), |r| format!("{r:.0}")),
        compute_rate.map_or(String::new(), |r| format!("{r:.0}")),
        write_rate.map_or(String::new(), |r| format!("{r:.0}")),
        modifications_counts.storage_tries,
        modifications_counts.contracts_trie,
        modifications_counts.classes_trie,
        modifications_counts.emptied_storage_leaves,
        emptied_leaves_percentage.map_or(String::new(), |p| format!(" ({p:.2}%)")),
        witness_log = witness_log,
    );
}
