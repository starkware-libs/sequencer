use std::error::Error;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::committer_types::{CommitBlockRequest, CommitBlockResponse};
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::hash::PoseidonHash;
use starknet_committer::db::forest_trait::{ForestMetadata, ForestMetadataType};
use starknet_committer::db::mock_forest_storage::MockForestStorage;
use starknet_committer::db::serde_db_utils::{deserialize_felt_no_packing, DbBlockNumber};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbValue, Storage};
use tracing::error;

pub type ApolloStorage = MapStorage;
pub type ApolloCommitter = Committer<ApolloStorage>;

pub trait StorageConstructor {
    fn create_storage() -> Self;
}

impl StorageConstructor for ApolloStorage {
    fn create_storage() -> Self {
        Self::default()
    }
}

/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: Storage + StorageConstructor> {
    /// Storage for forest operations.
    forest_storage: MockForestStorage<S>,
    /// Committer config.
    #[allow(dead_code)]
    config: CommitterConfig,
    /// The next block number to commit.
    offset: BlockNumber,
}

impl<S: Storage + StorageConstructor> Committer<S> {
    pub async fn new(config: CommitterConfig) -> Self {
        let mut forest_storage = MockForestStorage { storage: S::create_storage() };
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

        if height < self.offset {
            // Request to commit an old height.
            // Might be ok if the caller didn't get the results properly.
            let input_state_diff_commitment =
                state_diff_commitment.unwrap_or_else(|| calculate_state_diff_hash(&state_diff));
            let stored_state_diff_commitment = self.load_state_diff_commitment(height).await?;
            // Verify the input state diff matches the stored one by comparing the commitments.
            if input_state_diff_commitment != stored_state_diff_commitment {
                return Err(CommitterError::InvalidStateDiffCommitment {
                    input_commitment: input_state_diff_commitment,
                    stored_commitment: stored_state_diff_commitment,
                    height,
                });
            }
            // Returns the precomputed global root.
            let db_state_root = self.load_global_root(height).await?;
            return Ok(CommitBlockResponse { state_root: db_state_root });
        }

        // Happy flow. Commits the state diff and returns the computed global root.
        unimplemented!()
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
