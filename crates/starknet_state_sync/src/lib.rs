pub mod config;
pub mod runner;
#[cfg(test)]
mod test;

use std::cmp::min;

use async_trait::async_trait;
use futures::channel::mpsc::{channel, Sender};
use futures::SinkExt;
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::compiled_class::CasmStorageReader;
use papyrus_storage::db::TransactionKind;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::{StateReader, StateStorageReader};
use papyrus_storage::{StorageReader, StorageTxn};
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_sequencer_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use starknet_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use starknet_state_sync_types::errors::StateSyncError;
use starknet_state_sync_types::state_sync_types::{StateSyncResult, SyncBlock};
use starknet_types_core::felt::Felt;

use crate::config::StateSyncConfig;
use crate::runner::StateSyncRunner;

const BUFFER_SIZE: usize = 100000;

pub fn create_state_sync_and_runner(
    config: StateSyncConfig,
    class_manager_client: SharedClassManagerClient,
) -> (StateSync, StateSyncRunner) {
    let (new_block_sender, new_block_receiver) = channel(BUFFER_SIZE);
    let (state_sync_runner, storage_reader) =
        StateSyncRunner::new(config, new_block_receiver, class_manager_client);
    (StateSync { storage_reader, new_block_sender }, state_sync_runner)
}

pub struct StateSync {
    storage_reader: StorageReader,
    new_block_sender: Sender<SyncBlock>,
}

// TODO(shahak): Have StateSyncRunner call StateSync instead of the opposite once we stop supporting
// papyrus executable and can move the storage into StateSync.
#[async_trait]
impl ComponentRequestHandler<StateSyncRequest, StateSyncResponse> for StateSync {
    async fn handle_request(&mut self, request: StateSyncRequest) -> StateSyncResponse {
        match request {
            StateSyncRequest::GetBlock(block_number) => {
                StateSyncResponse::GetBlock(self.get_block(block_number).map(Box::new))
            }
            StateSyncRequest::AddNewBlock(sync_block) => StateSyncResponse::AddNewBlock(
                self.new_block_sender.send(*sync_block).await.map_err(StateSyncError::from),
            ),
            StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key) => {
                StateSyncResponse::GetStorageAt(self.get_storage_at(
                    block_number,
                    contract_address,
                    storage_key,
                ))
            }
            StateSyncRequest::GetNonceAt(block_number, contract_address) => {
                StateSyncResponse::GetNonceAt(self.get_nonce_at(block_number, contract_address))
            }
            StateSyncRequest::GetClassHashAt(block_number, contract_address) => {
                StateSyncResponse::GetClassHashAt(
                    self.get_class_hash_at(block_number, contract_address),
                )
            }
            StateSyncRequest::GetCompiledClassDeprecated(block_number, class_hash) => {
                StateSyncResponse::GetCompiledClassDeprecated(
                    self.get_compiled_class_deprecated(block_number, class_hash),
                )
            }
            StateSyncRequest::GetLatestBlockNumber() => {
                StateSyncResponse::GetLatestBlockNumber(self.get_latest_block_number())
            }
        }
    }
}

impl StateSync {
    fn get_block(&self, block_number: BlockNumber) -> StateSyncResult<Option<SyncBlock>> {
        let txn = self.storage_reader.begin_ro_txn()?;
        let block_header = txn.get_block_header(block_number)?;
        let Some(block_transaction_hashes) = txn.get_block_transaction_hashes(block_number)? else {
            return Ok(None);
        };
        let Some(thin_state_diff) = txn.get_state_diff(block_number)? else {
            return Ok(None);
        };
        let Some(block_header) = block_header else {
            return Ok(None);
        };
        Ok(Some(SyncBlock {
            state_diff: thin_state_diff,
            block_header_without_hash: block_header.block_header_without_hash,
            transaction_hashes: block_transaction_hashes,
        }))
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

        verify_contract_deployed(&state_reader, state_number, contract_address)?;

        let res = state_reader.get_storage_at(state_number, &contract_address, &storage_key)?;

        Ok(res)
    }

    fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncResult<Nonce> {
        let txn = self.storage_reader.begin_ro_txn()?;
        verify_synced_up_to(&txn, block_number)?;

        let state_number = StateNumber::unchecked_right_after_block(block_number);
        let state_reader = txn.get_state_reader()?;

        verify_contract_deployed(&state_reader, state_number, contract_address)?;

        let res = state_reader
            .get_nonce_at(state_number, &contract_address)?
            .ok_or(StateSyncError::ContractNotFound(contract_address))?;

        Ok(res)
    }

    fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncResult<ClassHash> {
        let txn = self.storage_reader.begin_ro_txn()?;
        verify_synced_up_to(&txn, block_number)?;

        let state_number = StateNumber::unchecked_right_after_block(block_number);
        let state_reader = txn.get_state_reader()?;
        let class_hash = state_reader
            .get_class_hash_at(state_number, &contract_address)?
            .ok_or(StateSyncError::ContractNotFound(contract_address))?;
        Ok(class_hash)
    }

    fn get_compiled_class_deprecated(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncResult<ContractClass> {
        let txn = self.storage_reader.begin_ro_txn()?;
        verify_synced_up_to(&txn, block_number)?;

        let state_reader = txn.get_state_reader()?;

        // Check if this class exists in the Cairo1 classes table.
        if let Some(class_definition_block_number) =
            state_reader.get_class_definition_block_number(&class_hash)?
        {
            if class_definition_block_number > block_number {
                return Err(StateSyncError::ClassNotFound(class_hash));
            }

            let (option_casm, option_sierra) = txn.get_casm_and_sierra(&class_hash)?;

            // Check if both options are `Some`. If not, since we verified the block number is
            // smaller than the casm marker, we return that the class doesnt exist.
            let (casm, sierra) =
                option_casm.zip(option_sierra).ok_or(StateSyncError::ClassNotFound(class_hash))?;
            let sierra_version = SierraVersion::extract_from_program(&sierra.sierra_program)?;
            return Ok(ContractClass::V1((casm, sierra_version)));
        }

        // Check if this class exists in the Cairo0 classes table.
        let state_number = StateNumber::unchecked_right_after_block(block_number);
        let deprecated_compiled_contract_class = state_reader
            .get_deprecated_class_definition_at(state_number, &class_hash)?
            .ok_or(StateSyncError::ClassNotFound(class_hash))?;
        Ok(ContractClass::V0(deprecated_compiled_contract_class))
    }

    fn get_latest_block_number(&self) -> StateSyncResult<Option<BlockNumber>> {
        let txn = self.storage_reader.begin_ro_txn()?;
        let latest_block_number = latest_synced_block(&txn)?;
        Ok(latest_block_number)
    }
}

fn verify_synced_up_to<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<(), StateSyncError> {
    if let Some(latest_block_number) = latest_synced_block(txn)? {
        if latest_block_number >= block_number {
            return Ok(());
        }
    }

    Err(StateSyncError::BlockNotFound(block_number))
}

fn latest_synced_block<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
) -> StateSyncResult<Option<BlockNumber>> {
    let latest_state_block_number = txn.get_state_marker()?.prev();
    if latest_state_block_number.is_none() {
        return Ok(None);
    }

    let latest_transaction_block_number = txn.get_body_marker()?.prev();
    if latest_transaction_block_number.is_none() {
        return Ok(None);
    }

    Ok(min(latest_state_block_number, latest_transaction_block_number))
}

fn verify_contract_deployed<Mode: TransactionKind>(
    state_reader: &StateReader<'_, Mode>,
    state_number: StateNumber,
    contract_address: ContractAddress,
) -> Result<(), StateSyncError> {
    // Contract address 0x1 is a special address, it stores the block
    // hashes. Contracts are not deployed to this address.
    if contract_address != BLOCK_HASH_TABLE_ADDRESS {
        // check if the contract is deployed
        state_reader
            .get_class_hash_at(state_number, &contract_address)?
            .ok_or(StateSyncError::ContractNotFound(contract_address))?;
    };

    Ok(())
}

pub type LocalStateSyncServer =
    LocalComponentServer<StateSync, StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncServer = RemoteComponentServer<StateSyncRequest, StateSyncResponse>;

impl ComponentStarter for StateSync {}
