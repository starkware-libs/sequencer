//! Module which provides easy access to read or write consensus state from the node's storage.
use std::fmt::Debug;

use apollo_storage::consensus::{ConsensusStorageReader, ConsensusStorageWriter, LastVotedMarker};
use apollo_storage::{open_storage, StorageConfig, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum HeightVotedStorageError {
    #[error(transparent)]
    StorageError(#[from] apollo_storage::StorageError),
}

#[cfg_attr(test, mockall::automock)]
pub(crate) trait HeightVotedStorage: Debug + Send + Sync {
    fn get_prev_voted_height(&self) -> Result<Option<BlockNumber>, HeightVotedStorageError>;
    fn set_prev_voted_height(&mut self, height: BlockNumber)
    -> Result<(), HeightVotedStorageError>;
}

struct HeightVotedStorageImpl {
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
}

pub(crate) fn get_voted_height_storage(config: StorageConfig) -> impl HeightVotedStorage {
    let (storage_reader, storage_writer) = open_storage(config).expect("Failed to open storage");
    HeightVotedStorageImpl { storage_reader, storage_writer }
}

impl HeightVotedStorage for HeightVotedStorageImpl {
    fn get_prev_voted_height(&self) -> Result<Option<BlockNumber>, HeightVotedStorageError> {
        let last_voted_marker = self.storage_reader.begin_ro_txn()?.get_last_voted_marker()?;
        match last_voted_marker {
            Some(last_voted_marker) => Ok(Some(last_voted_marker.height)),
            None => Ok(None),
        }
    }

    fn set_prev_voted_height(
        &mut self,
        height: BlockNumber,
    ) -> Result<(), HeightVotedStorageError> {
        // TODO(guy.f): Do we want to check if the height is greater than the current height
        // (defensive programming)?
        self.storage_writer.begin_rw_txn()?.set_last_voted_marker(&LastVotedMarker { height })?;
        Ok(())
    }
}

// We must implement Debug for HeightVotedStorageImpl to allow it being used as a member in structs
// that are Debug.
impl Debug for HeightVotedStorageImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeightVotedStorageImpl").finish()
    }
}
