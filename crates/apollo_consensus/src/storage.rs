//! Module which provides easy access to read or write consensus state from the node's storage.
use std::fmt::Debug;

use apollo_storage::consensus::{ConsensusStorageReader, ConsensusStorageWriter, LastVotedMarker};
use apollo_storage::{open_storage, StorageConfig, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;

#[cfg(test)]
#[path = "storage_test.rs"]
mod storage_test;

/// Possible errors when interacting the the height voted state.
#[derive(thiserror::Error, Debug)]
pub enum HeightVotedStorageError {
    /// Errors coming from the underlying storage.
    #[error(transparent)]
    StorageError(#[from] apollo_storage::StorageError),
    /// The storage state is invalid (e.g. trying to set a lower height than the current one).
    #[error("Inconsistent storage state: {error_msg}")]
    InconsistentStorageState {
        #[allow(missing_docs)]
        error_msg: String,
    },
}

/// Trait for interacting with the height voted state.
#[cfg_attr(test, mockall::automock)]
pub trait HeightVotedStorageTrait: Debug + Send + Sync {
    /// Returns the last height on which the node voted.
    // TODO(guy.f): Remove in the following PR.
    #[allow(dead_code)]
    fn get_prev_voted_height(&self) -> Result<Option<BlockNumber>, HeightVotedStorageError>;
    /// Sets the last height on which the node voted.
    fn set_prev_voted_height(&mut self, height: BlockNumber)
    -> Result<(), HeightVotedStorageError>;
}

struct HeightVotedStorage {
    // TODO(guy.f): Remove in the following PR.
    #[allow(dead_code)]
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
}

pub(crate) fn get_voted_height_storage(config: StorageConfig) -> impl HeightVotedStorageTrait {
    let (storage_reader, storage_writer) = open_storage(config).expect("Failed to open storage");
    HeightVotedStorage { storage_reader, storage_writer }
}

impl HeightVotedStorageTrait for HeightVotedStorage {
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
        let txn = self.storage_writer.begin_rw_txn()?;
        let last_voted_marker_from_storage = txn.get_last_voted_marker()?;
        let last_voted_marker_to_write = LastVotedMarker { height };
        if let Some(last_voted_marker_from_storage) = last_voted_marker_from_storage {
            if last_voted_marker_to_write < last_voted_marker_from_storage {
                return Err(HeightVotedStorageError::InconsistentStorageState {
                    error_msg: format!(
                        "Last voted height in storage {} is higher than the updated last voted \
                         height to write {}",
                        last_voted_marker_from_storage.height, last_voted_marker_to_write.height
                    ),
                });
            }
        }
        txn.set_last_voted_marker(&last_voted_marker_to_write)?.commit()?;
        Ok(())
    }
}

// We must implement Debug for HeightVotedStorageImpl to allow it being used as a member in structs
// that are Debug.
impl Debug for HeightVotedStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeightVotedStorageImpl").finish()
    }
}
