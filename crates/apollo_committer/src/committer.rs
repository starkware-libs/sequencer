use apollo_committer_config::config::CommitterConfig;
use starknet_api::block::BlockNumber;
use starknet_committer::db::facts_db::db::MockForestStorage;
use starknet_committer::db::forest_trait::{DbBlockNumber, ForestMetadata, ForestMetadataType};
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::Storage;

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
}

pub async fn create_committer(config: CommitterConfig) -> Committer<MapStorage> {
    let forest_storage = MockForestStorage { storage: MapStorage::default() };
    Committer::new(forest_storage, config).await
}
