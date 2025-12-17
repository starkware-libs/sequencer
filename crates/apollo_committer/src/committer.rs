use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::errors::{CommitterError, CommitterResult};
use starknet_api::block::BlockNumber;
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::db::facts_db::db::MockForestStorage;
use starknet_committer::db::forest_trait::{DbBlockNumber, ForestMetadata, ForestMetadataType};
use starknet_patricia_storage::db_object::{DBObject, EmptyDeserializationContext};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::Storage;
use tracing::error;

#[allow(dead_code)]
enum FeltValueMetadata {
    StateDiffCommitment(DbBlockNumber),
    StateRoot(DbBlockNumber),
}

impl From<FeltValueMetadata> for ForestMetadataType {
    fn from(value: FeltValueMetadata) -> Self {
        match value {
            FeltValueMetadata::StateDiffCommitment(block_number) => {
                ForestMetadataType::StateDiffHash(block_number)
            }
            FeltValueMetadata::StateRoot(block_number) => {
                ForestMetadataType::StateRoot(block_number)
            }
        }
    }
}

#[allow(dead_code)]
pub struct Committer<S: Storage> {
    forest_storage: MockForestStorage<S>,
    config: CommitterConfig,
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
    async fn read_metadata(
        &mut self,
        metadata: FeltValueMetadata,
    ) -> CommitterResult<StarknetStorageValue> {
        let metadata_type: ForestMetadataType = metadata.into();
        let db_value = self
            .forest_storage
            .read_metadata(metadata_type.clone())
            .await
            .map_err(|err| self.map_internal_error(err))?
            .ok_or(CommitterError::MissingMetadata(metadata_type))?;
        StarknetStorageValue::deserialize(&db_value, &EmptyDeserializationContext)
            .map_err(|err| self.map_internal_error(err))
    }

    fn map_internal_error<E: ToString>(&self, err: E) -> CommitterError {
        let error_message = err.to_string();
        error!("Error committing block number {0}: {error_message}", self.offset);
        CommitterError::Internal { height: self.offset, message: error_message }
    }
}

pub async fn create_committer(config: CommitterConfig) -> Committer<MapStorage> {
    let forest_storage = MockForestStorage { storage: MapStorage::default() };
    Committer::new(forest_storage, config).await
}
