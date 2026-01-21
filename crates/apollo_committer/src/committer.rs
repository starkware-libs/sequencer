use std::collections::HashMap;
use std::error::Error;
use std::marker::PhantomData;
use std::path::Path;

use apollo_committer_config::config::{CommitterConfig, CommitterStorageConfig};
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
use starknet_committer::block_committer::timing_util::TimeMeasurement;
use starknet_committer::db::forest_trait::{
    ForestMetadata,
    ForestMetadataType,
    ForestReader,
    ForestWriterWithMetadata,
};
use starknet_committer::db::mock_forest_storage::{MockForestStorage, MockIndexInitialRead};
use starknet_committer::db::serde_db_utils::{
    deserialize_felt_no_packing,
    serialize_felt_no_packing,
    DbBlockNumber,
};
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use starknet_patricia_storage::storage_trait::{DbValue, Storage};
use tracing::{debug, error, info, warn};

use crate::metrics::register_metrics;

#[cfg(test)]
#[path = "committer_test.rs"]
mod committer_test;

pub type ApolloStorage = RocksDbStorage;

// TODO(Yoav): Move this to committer_test.rs and use index db reader.
pub struct CommitBlockMock;

#[async_trait]
impl CommitBlockTrait for CommitBlockMock {
    /// Sets the class trie root hash to the first class hash in the state diff.
    async fn commit_block<Reader: ForestReader + Send>(
        input: Input<Reader::InitialReadContext>,
        _trie_reader: &mut Reader,
        _time_measurement: Option<&mut TimeMeasurement>,
    ) -> BlockCommitmentResult<FilledForest> {
        let root_class_hash = match input.state_diff.class_hash_to_compiled_class_hash.iter().next()
        {
            Some(class_hash) => HashOutput(class_hash.0.0),
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

pub type ApolloCommitter = Committer<ApolloStorage, CommitBlockMock>;

pub trait StorageConstructor: Storage {
    fn create_storage(storage_config: CommitterStorageConfig) -> Self;
}

#[cfg(test)]
impl StorageConstructor for starknet_patricia_storage::map_storage::MapStorage {
    fn create_storage(_storage_config: CommitterStorageConfig) -> Self {
        Self::default()
    }
}

impl StorageConstructor for RocksDbStorage {
    fn create_storage(storage_config: CommitterStorageConfig) -> Self {
        Self::open(Path::new(&storage_config.path), RocksDbOptions::default()).unwrap()
    }
}

/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: StorageConstructor, CB: CommitBlockTrait> {
    /// Storage for forest operations.
    forest_storage: MockForestStorage<S>,
    /// Committer config.
    config: CommitterConfig,
    /// The next block number to commit.
    offset: BlockNumber,
    // Allow define the generic type CB and not use it.
    phantom: PhantomData<CB>,
}

impl<S: StorageConstructor, CB: CommitBlockTrait> Committer<S, CB> {
    pub async fn new(config: CommitterConfig) -> Self {
        let mut forest_storage =
            MockForestStorage { storage: S::create_storage(config.storage_config.clone()) };
        let offset = Self::load_offset_or_panic(&mut forest_storage).await;
        info!("Initializing committer with offset: {offset}");
        Self { forest_storage, config, offset, phantom: PhantomData }
    }

    /// Commits a block to the forest.
    /// In the happy flow, the given height equals to the committer offset.
    pub async fn commit_block(
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

        let state_diff_commitment =
            state_diff_commitment.unwrap_or_else(|| calculate_state_diff_hash(&state_diff));
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
            let db_state_root = self.load_global_root(height).await?;
            return Ok(CommitBlockResponse { state_root: db_state_root });
        }

        // Happy flow. Commits the state diff and returns the computed global root.
        debug!("Committing block number {height} with state diff {state_diff_commitment:?}");
        let (filled_forest, global_root) = self.commit_state_diff(state_diff).await?;
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
        self.forest_storage
            .write_with_metadata(&filled_forest, metadata)
            .await
            .map_err(|err| self.map_internal_error(err))?;
        self.offset = next_offset;
        Ok(CommitBlockResponse { state_root: global_root })
    }

    /// Applies the given state diff to revert the changes of the given height.
    pub async fn revert_block(
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
        let (filled_forest, revert_global_root) =
            self.commit_state_diff(reversed_state_diff).await?;

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
        self.forest_storage
            .write_with_metadata(&filled_forest, metadata)
            .await
            .map_err(|err| self.map_internal_error(err))?;
        self.offset = last_committed_block;
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

    async fn load_offset_or_panic(forest_storage: &mut MockForestStorage<S>) -> BlockNumber {
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

    async fn commit_state_diff(
        &mut self,
        state_diff: ThinStateDiff,
    ) -> CommitterResult<(FilledForest, GlobalRoot)> {
        let input = Input {
            state_diff: state_diff.into(),
            initial_read_context: MockIndexInitialRead {},
            config: self.config.reader_config.clone(),
        };
        let time_measurement = None;
        let filled_forest = CB::commit_block(input, &mut self.forest_storage, time_measurement)
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
        register_metrics();
    }
}
