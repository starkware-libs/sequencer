//! Interface for handling data related to Consensus state.
//!
//! The consensus state contains consensus state that needs to be persisted between restarts.
//!
//! # Example
//! ```
//! use apollo_storage::open_storage;
//! # use apollo_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! use apollo_storage::consensus::{ConsensusStorageReader, ConsensusStorageWriter, LastVotedMarker};
//! use starknet_api::block::BlockNumber;
//!
//! # // Example config only, real code should set these values to correct values.
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId::Mainnet,
//! #     enforce_file_exists: false,
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! #     max_readers: 1 << 13, // 8K readers
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! let (reader, mut writer) = open_storage(storage_config)?;
//! let last_voted_marker = LastVotedMarker {
//!     height: BlockNumber(42),
//! };
//! writer
//!     .begin_rw_txn()?                                                   // Start a RW transaction.
//!     .set_last_voted_marker(&last_voted_marker)?
//!     .commit()?;                                                         // Commit the transaction.
//! let last_voted_marker_from_storage = reader.begin_ro_txn()?.get_last_voted_marker()?;
//! assert_eq!(Some(last_voted_marker), last_voted_marker_from_storage);
//! # Ok::<(), apollo_storage::StorageError>(())

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;

use crate::db::table_types::Table;
use crate::db::{TransactionKind, RW};
use crate::{StorageResult, StorageTxn};

/// Information about the last vote sent out by consensus.
#[derive(Debug, Clone, Eq, PartialEq, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LastVotedMarker {
    /// The last height in which consensus voted.
    pub height: BlockNumber,
    // TODO(guy.f): Add `round: Round`. We should add it here for future possible use however the
    // type Round is defined in apollo_consensus crate which would make a circular dependency.
}

#[allow(missing_docs)]
pub trait ConsensusStorageReader {
    fn get_last_voted_marker(&self) -> StorageResult<Option<LastVotedMarker>>;
}

#[allow(missing_docs)]
pub trait ConsensusStorageWriter
where
    Self: Sized,
{
    fn set_last_voted_marker(self, last_voted_marker: &LastVotedMarker) -> StorageResult<Self>;
    fn clear_last_voted_marker(self) -> StorageResult<Self>;
}

impl<Mode: TransactionKind> ConsensusStorageReader for StorageTxn<'_, Mode> {
    fn get_last_voted_marker(&self) -> StorageResult<Option<LastVotedMarker>> {
        let table = self.open_table(&self.tables.last_voted_marker)?;
        Ok(table.get(&self.txn, &())?)
    }
}

impl ConsensusStorageWriter for StorageTxn<'_, RW> {
    fn set_last_voted_marker(self, last_voted_marker: &LastVotedMarker) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.last_voted_marker)?;
        table.upsert(&self.txn, &(), last_voted_marker)?;
        Ok(self)
    }

    fn clear_last_voted_marker(self) -> StorageResult<Self> {
        let table = self.open_table(&self.tables.last_voted_marker)?;
        table.delete(&self.txn, &())?;
        Ok(self)
    }
}
