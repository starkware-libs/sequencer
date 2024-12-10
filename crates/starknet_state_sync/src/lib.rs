pub mod config;
pub mod runner;

use async_trait::async_trait;
use futures::channel::mpsc::{channel, Sender};
use futures::SinkExt;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_state_sync_types::communication::{
    StateSyncRequest,
    StateSyncResponse,
    StateSyncResult,
};
use starknet_state_sync_types::errors::StateSyncError;
use starknet_state_sync_types::state_sync_types::SyncBlock;

use crate::config::StateSyncConfig;
use crate::runner::StateSyncRunner;

const BUFFER_SIZE: usize = 100000;

pub fn create_state_sync_and_runner(config: StateSyncConfig) -> (StateSync, StateSyncRunner) {
    let (new_block_sender, new_block_receiver) = channel(BUFFER_SIZE);
    let (state_sync_runner, storage_reader) = StateSyncRunner::new(config, new_block_receiver);
    (StateSync { storage_reader, new_block_sender }, state_sync_runner)
}

pub struct StateSync {
    storage_reader: StorageReader,
    new_block_sender: Sender<(BlockNumber, SyncBlock)>,
}

// TODO(shahak): Have StateSyncRunner call StateSync instead of the opposite once we stop supporting
// papyrus executable and can move the storage into StateSync.
#[async_trait]
impl ComponentRequestHandler<StateSyncRequest, StateSyncResponse> for StateSync {
    async fn handle_request(&mut self, request: StateSyncRequest) -> StateSyncResponse {
        match request {
            StateSyncRequest::GetBlock(block_number) => {
                StateSyncResponse::GetBlock(self.get_block(block_number))
            }
            StateSyncRequest::AddNewBlock(block_number, sync_block) => {
                StateSyncResponse::AddNewBlock(
                    self.new_block_sender
                        .send((block_number, sync_block))
                        .await
                        .map_err(StateSyncError::from),
                )
            }
        }
    }
}

impl StateSync {
    fn get_block(&self, block_number: BlockNumber) -> StateSyncResult<Option<SyncBlock>> {
        let txn = self.storage_reader.begin_ro_txn()?;
        if let Some(block_transaction_hashes) = txn.get_block_transaction_hashes(block_number)? {
            if let Some(thin_state_diff) = txn.get_state_diff(block_number)? {
                return Ok(Some(SyncBlock {
                    state_diff: thin_state_diff,
                    transaction_hashes: block_transaction_hashes,
                }));
            }
        }

        Ok(None)
    }
}

pub type LocalStateSyncServer =
    LocalComponentServer<StateSync, StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncServer = RemoteComponentServer<StateSyncRequest, StateSyncResponse>;

impl ComponentStarter for StateSync {}
