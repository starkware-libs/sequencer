use std::collections::HashMap;
use std::path::PathBuf;

use apollo_committer_config::config::ApolloStorage;
use starknet_api::block::BlockNumber;
use starknet_committer::db::forest_trait::{
    ForestMetadata,
    ForestMetadataType,
    ForestWriter,
    StorageInitializer,
};
use starknet_committer::db::index_db::db::IndexDb;
use starknet_committer::db::serde_db_utils::DbBlockNumber;
use starknet_patricia_storage::storage_trait::DbValue;

use crate::committer::StorageConstructor;

type ApolloCommitterDb = IndexDb<ApolloStorage>;

/// Pre-initializes the committer's RocksDB with `initial_block_number` as its commitment offset.
///
/// Used in integration tests when the genesis block is not block 0. The committer normally starts
/// from offset 0 and rejects commitment requests for higher block numbers. By writing the offset
/// here before the node starts, the committer will accept blocks starting at
/// `initial_block_number`.
pub async fn initialize_committer_storage(db_path: PathBuf, initial_block_number: BlockNumber) {
    if initial_block_number.0 == 0 {
        return;
    }

    let storage = ApolloStorage::create_storage(db_path, Default::default());
    let mut forest_storage = ApolloCommitterDb::new(storage);

    let value = DbValue(DbBlockNumber(initial_block_number).serialize().to_vec());
    let mut updates = HashMap::new();
    ApolloCommitterDb::insert_metadata(&mut updates, ForestMetadataType::CommitmentOffset, value);
    forest_storage.write_updates(updates).await;
}
