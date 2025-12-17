use std::collections::HashMap;
use std::error::Error;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::committer_types::{CommitBlockRequest, CommitBlockResponse};
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::hash::PoseidonHash;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::Input;
use starknet_committer::db::forest_trait::{
    ForestMetadata,
    ForestMetadataType,
    ForestWriterWithMetadata,
};
use starknet_committer::db::mock_forest_storage::{MockForestStorage, MockIndexInitialRead};
use starknet_committer::db::serde_db_utils::{
    deserialize_felt_no_packing,
    serialize_felt_no_packing,
    DbBlockNumber,
};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbValue, Storage};
use tracing::error;

pub type ApolloCommitter = Committer<MapStorage>;

/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: Storage> {
    /// Storage for forest operations.
    forest_storage: MockForestStorage<S>,
    /// Committer config.
    config: CommitterConfig,
    /// The next block number to commit.
    offset: BlockNumber,
}

impl<S: Storage> Committer<S> {
    pub async fn new(mut forest_storage: MockForestStorage<S>, config: CommitterConfig) -> Self {
        let db_offset = forest_storage
            .read_metadata(ForestMetadataType::CommitmentOffset)
            .await
            .expect("Failed to read commitment offset");

        let offset = db_offset
            .map(|value| {
                let array_value: [u8; 8] = value.0.try_into().unwrap_or_else(|value| {
                    panic!("Failed to deserialize commitment offset from {value:?}")
                });
                DbBlockNumber::deserialize(array_value)
            })
            .unwrap_or_default()
            .0;

        Self { forest_storage, config, offset }
    }

    /// Commits a block to the forest.
    /// In the happy flow, the given height equals to the committer offset.
    pub async fn commit_block(
        &mut self,
        CommitBlockRequest { state_diff, state_diff_commitment, height }: CommitBlockRequest,
    ) -> CommitterResult<CommitBlockResponse> {
        if height > self.offset {
            // Request to commit a future height.
            // Returns an error, indicating the committer has a hole in the state diff series.
            return Err(CommitterError::HeightHole {
                input_height: height,
                committer_offset: self.offset,
            });
        }

        let state_diff_commitment =
            state_diff_commitment.unwrap_or_else(|| calculate_state_diff_hash(&state_diff));
        if height < self.offset {
            // Request to commit an old height.
            // Might be ok if the caller didn't get the results properly.
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
        let input = Input {
            state_diff: state_diff.into(),
            initial_read_context: MockIndexInitialRead {},
            config: self.config.reader_config.clone(),
        };
        let filled_forest = commit_block(input, &mut self.forest_storage, None)
            .await
            .map_err(|err| self.map_internal_error(err))?;
        let global_root = filled_forest.state_roots().global_root();

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
        self.forest_storage
            .write_with_metadata(&filled_forest, metadata)
            .await
            .map_err(|err| self.map_internal_error(err))?;
        self.offset = next_offset;
        Ok(CommitBlockResponse { state_root: global_root })
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

    // Reads metadata from the storage, returns an error if it is not found.
    async fn read_metadata(&mut self, metadata: ForestMetadataType) -> CommitterResult<DbValue> {
        self.forest_storage
            .read_metadata(metadata.clone())
            .await
            .map_err(|err| self.map_internal_error(err))?
            .ok_or(CommitterError::MissingMetadata(metadata))
    }

    fn map_internal_error<E: Error>(&self, err: E) -> CommitterError {
        let error_message = format!("{err:?}: {err}");
        error!("Error committing block number {0}. {error_message}.", self.offset);
        CommitterError::Internal { height: self.offset, message: error_message }
    }
}

pub async fn create_committer(config: CommitterConfig) -> ApolloCommitter {
    let forest_storage = MockForestStorage { storage: MapStorage::default() };
    Committer::new(forest_storage, config).await
}
