pub mod config;
pub mod runner;

use async_trait::async_trait;
use futures::channel::mpsc::{channel, Sender};
use futures::SinkExt;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use starknet_state_sync_types::errors::StateSyncError;
use starknet_state_sync_types::state_sync_types::{StateSyncResult, SyncBlock};
use starknet_types_core::felt::Felt;

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
            StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key) => {
                StateSyncResponse::GetStorageAt(self.get_storage_at(
                    block_number,
                    contract_address,
                    storage_key,
                ))
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
                    block_number,
                    state_diff: thin_state_diff,
                    transaction_hashes: block_transaction_hashes,
                }));
            }
        }

        Ok(None)
    }

    fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncResult<Felt> {
        let txn = self.storage_reader.begin_ro_txn()?;
        verify_synced_up_to(&txn, block_number)?;

        let state_number = StateNumber::unchecked_right_after_block(block_number);
        let state_reader = txn.get_state_reader()?;
        let res = state_reader.get_storage_at(state_number, &contract_address, &storage_key)?;

        // If the contract is not deployed, res will be 0. Checking if that's the case so
        // that we'll return an error instead.
        // Contract address 0x1 is a special address, it stores the block
        // hashes. Contracts are not deployed to this address.
        if res == Felt::default() && contract_address != BLOCK_HASH_TABLE_ADDRESS {
            // check if the contract exists
            state_reader
                .get_class_hash_at(state_number, &contract_address)?
                .ok_or(StateSyncError::ContractNotFound(contract_address))?;
        };

        Ok(res)
    }
}

fn verify_synced_up_to<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<(), StateSyncError> {
    if let Some(latest_block_number) = txn.get_state_marker()?.prev() {
        if latest_block_number >= block_number {
            return Ok(());
        }
    }

    Err(StateSyncError::BlockNotFound(block_number))
}

pub type LocalStateSyncServer =
    LocalComponentServer<StateSync, StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncServer = RemoteComponentServer<StateSyncRequest, StateSyncResponse>;

impl ComponentStarter for StateSync {}
