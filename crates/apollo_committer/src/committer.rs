use std::collections::HashMap;
use std::error::Error;
use std::marker::PhantomData;
use std::path::PathBuf;

use apollo_committer_config::config::{ApolloStorage, CommitterConfig};
use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::hash::{HashOutput, PoseidonHash};
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::commit::{BlockCommitmentResult, CommitBlockTrait};
use starknet_committer::block_committer::input::Input;
use starknet_committer::block_committer::measurements_util::{
    Action,
    BlockDurations,
    BlockMeasurement,
    BlockModificationsCounts,
    MeasurementsTrait,
    SingleBlockMeasurements,
};
use starknet_committer::db::forest_trait::{
    EmptyInitialReadContext,
    ForestMetadataType,
    ForestReader,
    ForestStorageWithEmptyReadContext,
};
use starknet_committer::db::index_db::IndexDb;
use starknet_committer::db::serde_db_utils::{
    deserialize_felt_no_packing,
    serialize_felt_no_packing,
    DbBlockNumber,
};
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia_storage::map_storage::CachedStorage;
use starknet_patricia_storage::rocksdb_storage::RocksDbStorage;
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
    EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK,
    READ_DURATION_PER_BLOCK,
    TOTAL_BLOCK_DURATION,
    TOTAL_BLOCK_DURATION_PER_MODIFICATION,
    WRITE_DURATION_PER_BLOCK,
};

#[cfg(test)]
#[path = "committer_test.rs"]
mod committer_test;

// TODO(Yoav): Move this to committer_test.rs.
pub struct CommitBlockMock;

#[async_trait]
impl CommitBlockTrait for CommitBlockMock {
    /// Sets the class trie root hash to the first class hash in the state diff (sorted
    /// deterministically).
    async fn commit_block<Reader: ForestReader + Send, M: MeasurementsTrait + Send>(
        input: Input<Reader::InitialReadContext>,
        _trie_reader: &mut Reader,
        _measurements: &mut M,
    ) -> BlockCommitmentResult<FilledForest> {
        // Sort class hashes deterministically to ensure all nodes get the same "first" class hash
        let mut sorted_class_hashes: Vec<_> =
            input.state_diff.class_hash_to_compiled_class_hash.keys().collect();
        sorted_class_hashes.sort();

        let root_class_hash = match sorted_class_hashes.first() {
            Some(class_hash) => HashOutput(class_hash.0),
            None => HashOutput::ROOT_OF_EMPTY_TREE,
        };
        Ok(FilledForest {
            storage_tries: HashMap::new(),
            contracts_trie: FilledTreeImpl {
                tree_map: HashMap::new(),
                root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
            },
            classes_trie: FilledTreeImpl { tree_map: HashMap::new(), root_hash: root_class_hash },
        })
    }
}

pub type ApolloCommitterDb = IndexDb<ApolloStorage>;

pub struct BlockCommitter;
impl CommitBlockTrait for BlockCommitter {}

pub type ApolloCommitter = Committer<ApolloStorage, ApolloCommitterDb, BlockCommitter>;

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

/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: Storage, ForestDB, BlockCommitter>
where
    ForestDB: ForestStorageWithEmptyReadContext,
    BlockCommitter: CommitBlockTrait,
{
    /// Storage for forest operations.
    forest_storage: ForestDB,
    /// Committer config.
    config: CommitterConfig<S::Config>,
    /// The next block number to commit.
    offset: BlockNumber,
    // Allow define the generic type CB and not use it.
    phantom: PhantomData<BlockCommitter>,
}

impl<S, ForestDB, BlockCommitter> Committer<S, ForestDB, BlockCommitter>
where
    S: StorageConstructor,
    ForestDB: ForestStorageWithEmptyReadContext<Storage = S>,
    BlockCommitter: CommitBlockTrait,
{
    pub async fn new(config: CommitterConfig<S::Config>) -> Self {
        let storage = S::create_storage(config.db_path.clone(), config.storage_config.clone());
        let mut forest_storage = ForestDB::new(storage);
        let offset = Self::load_offset_or_panic(&mut forest_storage).await;
        info!("Initializing committer with offset: {offset}");
        Self { forest_storage, config, offset, phantom: PhantomData }
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
                info!("Committed block number {height} with state diff {state_diff_commitment:?}");
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
                    let calculated_commitment = calculate_state_diff_hash(&state_diff);
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
            None => calculate_state_diff_hash(&state_diff),
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
            return Ok(CommitBlockResponse { global_root: db_global_root });
        }

        // Happy flow. Commits the state diff and returns the computed global root.
        debug!("Committing block number {height} with state diff {state_diff_commitment:?}");
        let mut block_measurements = SingleBlockMeasurements::default();
        block_measurements.start_measurement(Action::EndToEnd);
        let (filled_forest, global_root) =
            self.commit_state_diff(state_diff, &mut block_measurements).await?;
        let next_offset = height.unchecked_next();
        let metadata = HashMap::from([
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
        ]);
        info!(
            "For block number {height}, writing filled forest to storage with metadata: \
             {metadata:?}"
        );
        block_measurements.start_measurement(Action::Write);
        let n_write_entries = self
            .forest_storage
            .write_with_metadata(&filled_forest, metadata)
            .await
            .map_err(|err| self.map_internal_error(err))?;
        block_measurements.attempt_to_stop_measurement(Action::Write, n_write_entries).ok();
        block_measurements.attempt_to_stop_measurement(Action::EndToEnd, 0).ok();
        update_metrics(height, &block_measurements.block_measurement);
        self.update_offset(next_offset);
        Ok(CommitBlockResponse { global_root })
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
        let (filled_forest, revert_global_root) =
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
        let n_write_entries = self
            .forest_storage
            .write_with_metadata(&filled_forest, metadata)
            .await
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
    ) -> CommitterResult<(FilledForest, GlobalRoot)> {
        let input = Input {
            state_diff: state_diff.into(),
            initial_read_context: ForestDB::InitialReadContext::create_empty(),
            config: self.config.reader_config.clone(),
        };
        let filled_forest =
            BlockCommitter::commit_block(input, &mut self.forest_storage, measurements)
                .await
                .map_err(|err| self.map_internal_error(err))?;
        let global_root = filled_forest.state_roots().global_root();
        Ok((filled_forest, global_root))
    }

    fn map_internal_error<E: Error>(&self, err: E) -> CommitterError {
        let error_message = format!("{err:?}: {err}");
        error!("Error committing block number {0}. {error_message}.", self.offset);
        CommitterError::Internal { height: self.offset, message: error_message }
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
fn update_metrics(
    height: BlockNumber,
    BlockMeasurement { n_reads, n_writes, durations, modifications_counts }: &BlockMeasurement,
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

    COUNT_STORAGE_TRIES_MODIFICATIONS_PER_BLOCK.record_lossy(modifications_counts.storage_tries);
    COUNT_CONTRACTS_TRIE_MODIFICATIONS_PER_BLOCK.record_lossy(modifications_counts.contracts_trie);
    COUNT_CLASSES_TRIE_MODIFICATIONS_PER_BLOCK.record_lossy(modifications_counts.classes_trie);
    COUNT_EMPTIED_LEAVES_PER_BLOCK.record_lossy(modifications_counts.emptied_storage_leaves);

    let emptied_leaves_percentage = if modifications_counts.storage_tries > 0 {
        let percentage = modifications_counts.emptied_storage_leaves as f64
            / modifications_counts.storage_tries as f64;
        EMPTIED_LEAVES_PERCENTAGE_PER_BLOCK.record_lossy(percentage);
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
) {
    info!(
        "Block {height} stats: durations in ms (total/read/compute/write): \
         {:.0}/{:.0}/{:.0}/{:.0}, total block duration per modification in µs: {}, rates \
         (read/compute/write): {}/{}/{}, modifications count \
         (storage_tries/contracts_trie/classes_trie/emptied_storage_leaves): {}/{}/{}/{}{}",
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
        emptied_leaves_percentage.map_or(String::new(), |p| format!(" ({p:.2}%)"))
    );
}
