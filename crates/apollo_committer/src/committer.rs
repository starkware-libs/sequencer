use apollo_committer_config::config::CommitterConfig;
use starknet_api::block::BlockNumber;
use starknet_committer::db::forest_trait::{ForestMetadata, ForestMetadataType};
use starknet_committer::db::mock_forest_storage::MockForestStorage;
use starknet_committer::db::serde_db_utils::DbBlockNumber;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_patricia_storage::storage_trait::Storage;

pub type ApolloCommitter = Committer<MapStorage>;

#[allow(dead_code)]
/// Apollo committer. Maintains the Starknet state tries in persistent storage.
pub struct Committer<S: Storage + Default> {
    /// Storage for forest operations.
    forest_storage: MockForestStorage<S>,
    /// Committer config.
    config: CommitterConfig,
    /// The next block number to commit.
    offset: BlockNumber,
}

impl<S: Storage + Default> Committer<S> {
    pub async fn new(config: CommitterConfig) -> Self {
        let mut forest_storage = MockForestStorage { storage: S::default() };
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
}
