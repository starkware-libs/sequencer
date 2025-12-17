use std::error::Error;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use starknet_api::block::BlockNumber;
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::hash::PoseidonHash;
use starknet_committer::db::forest_trait::{ForestMetadata, ForestMetadataType};
use starknet_committer::db::mock_forest_storage::MockForestStorage;
use starknet_committer::db::serde_db_utils::{deserialize_felt, DbBlockNumber};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::{DbValue, Storage};
use tracing::error;

#[allow(dead_code)]
/// Apollo committer. Applies commits and reverts to owned storage.
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
                DbBlockNumber::deserialize(&value.0)
                    .unwrap_or_else(|_| {
                        panic!("Failed to deserialize commitment offset from {value:?}")
                    })
                    .0
            })
            // Set default offset to 0 if no offset is found.
            .unwrap_or_default();
        Self { forest_storage, config, offset }
    }

    #[allow(dead_code)]
    async fn load_state_diff_commitment(
        &mut self,
        block_number: BlockNumber,
    ) -> CommitterResult<StateDiffCommitment> {
        let db_value = self
            .read_metadata(ForestMetadataType::StateDiffHash(DbBlockNumber(block_number)))
            .await?;
        Ok(StateDiffCommitment(PoseidonHash(deserialize_felt(&db_value))))
    }

    #[allow(dead_code)]
    async fn load_global_root(&mut self, block_number: BlockNumber) -> CommitterResult<GlobalRoot> {
        let db_value =
            self.read_metadata(ForestMetadataType::StateRoot(DbBlockNumber(block_number))).await?;
        Ok(GlobalRoot(deserialize_felt(&db_value)))
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

pub async fn create_committer(config: CommitterConfig) -> Committer<MapStorage> {
    let forest_storage = MockForestStorage { storage: MapStorage::default() };
    Committer::new(forest_storage, config).await
}
