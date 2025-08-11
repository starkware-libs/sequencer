pub mod config;
pub mod runner;
#[cfg(test)]
mod test;

use std::cmp::min;
use std::sync::Arc;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_infra::component_definitions::{ComponentRequestHandler, ComponentStarter};
use apollo_infra::component_server::{ConcurrentLocalComponentServer, RemoteComponentServer};
use apollo_starknet_client::reader::{StarknetFeederGatewayClient, StarknetReader};
use apollo_state_sync_types::communication::{StateSyncRequest, StateSyncResponse};
use apollo_state_sync_types::errors::StateSyncError;
use apollo_state_sync_types::state_sync_types::{StateSyncResult, SyncBlock};
use apollo_storage::body::BodyStorageReader;
use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::{StateReader, StateStorageReader};
use apollo_storage::{StorageReader, StorageTxn};
use async_trait::async_trait;
use futures::channel::mpsc::{channel, Sender};
use futures::SinkExt;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, BLOCK_HASH_TABLE_ADDRESS};
use starknet_api::state::{StateNumber, StorageKey};
use starknet_api::transaction::{Transaction, TransactionHash};
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
        StateSyncRunner::new(config.clone(), new_block_receiver, class_manager_client);
    (StateSync::new(storage_reader, new_block_sender, config), state_sync_runner)
}

#[derive(Clone)]
pub struct StateSync {
    storage_reader: StorageReader,
    new_block_sender: Sender<SyncBlock>,
    starknet_client: Option<Arc<dyn StarknetReader + Send + Sync>>,
}

impl StateSync {
    pub fn new(
        storage_reader: StorageReader,
        new_block_sender: Sender<SyncBlock>,
        config: StateSyncConfig,
    ) -> Self {
        let starknet_client = config.central_sync_client_config.map(|config| {
            let config = config.central_source_config;
            let starknet_client: Arc<dyn StarknetReader + Send + Sync> = Arc::new(
                StarknetFeederGatewayClient::new(
                    config.starknet_url.as_ref(),
                    config.http_headers,
                    // TODO(shahak): fill with a proper version, or allow not specifying the
                    // node version.
                    "",
                    config.retry_config,
                )
                .expect("Failed creating feeder gateway client"),
            );
            starknet_client
        });
        Self { storage_reader, new_block_sender, starknet_client }
    }
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
<<<<<<< HEAD
            StateSyncRequest::GetBlockHash(block_number) => {
                StateSyncResponse::GetBlockHash(self.get_block_hash(block_number))
            }
||||||| 38f03e1d0
=======
            StateSyncRequest::GetBlockHash(block_number) => {
                StateSyncResponse::GetBlockHash(self.get_block_hash(block_number).await)
            }
>>>>>>> origin/main-v0.14.0
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
            StateSyncRequest::GetLatestBlockNumber() => {
                StateSyncResponse::GetLatestBlockNumber(self.get_latest_block_number())
            }
            // TODO(shahak): Add tests for is_class_declared_at.
            StateSyncRequest::IsClassDeclaredAt(block_number, class_hash) => {
                StateSyncResponse::IsClassDeclaredAt(
                    self.is_class_declared_at(block_number, class_hash),
                )
            }
        }
    }
}

impl StateSync {
    fn get_block(&self, block_number: BlockNumber) -> StateSyncResult<SyncBlock> {
        let txn = self.storage_reader.begin_ro_txn()?;
<<<<<<< HEAD

        let block_header = txn
            .get_block_header(block_number)?
            .ok_or(StateSyncError::BlockNotFound(block_number))?;
        let block_transactions_with_hash = txn
            .get_block_transactions_with_hash(block_number)?
            .ok_or(StateSyncError::BlockNotFound(block_number))?;
        let thin_state_diff =
            txn.get_state_diff(block_number)?.ok_or(StateSyncError::BlockNotFound(block_number))?;
||||||| 38f03e1d0
        let block_header = txn.get_block_header(block_number)?;
        let Some(block_transactions_with_hash) =
            txn.get_block_transactions_with_hash(block_number)?
        else {
            return Ok(None);
        };
        let Some(thin_state_diff) = txn.get_state_diff(block_number)? else {
            return Ok(None);
        };
        let Some(block_header) = block_header else {
            return Ok(None);
        };
=======

        let block_not_found_err = Err(StateSyncError::BlockNotFound(block_number));
        let Some(block_header) = txn.get_block_header(block_number)? else {
            return block_not_found_err;
        };
        let Some(block_transactions_with_hash) =
            txn.get_block_transactions_with_hash(block_number)?
        else {
            return block_not_found_err;
        };
        let Some(thin_state_diff) = txn.get_state_diff(block_number)? else {
            return block_not_found_err;
        };
>>>>>>> origin/main-v0.14.0

        let mut l1_transaction_hashes: Vec<TransactionHash> = vec![];
        let mut account_transaction_hashes: Vec<TransactionHash> = vec![];
        for (tx, tx_hash) in block_transactions_with_hash {
            match tx {
                Transaction::L1Handler(_) => l1_transaction_hashes.push(tx_hash),
                _ => account_transaction_hashes.push(tx_hash),
            }
        }

        Ok(SyncBlock {
            state_diff: thin_state_diff,
            block_header_without_hash: block_header.block_header_without_hash,
            account_transaction_hashes,
            l1_transaction_hashes,
<<<<<<< HEAD
        })
    }

    fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncResult<BlockHash> {
        let txn = self.storage_reader.begin_ro_txn()?;
        let block_header = txn
            .get_block_header(block_number)?
            .ok_or(StateSyncError::BlockNotFound(block_number))?;
        Ok(block_header.block_hash)
||||||| 38f03e1d0
        }))
=======
        })
    }

    async fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncResult<BlockHash> {
        // Getting the next block because the Sync block only contains parent hash.
        match (self.get_block(block_number.unchecked_next()), self.starknet_client.as_ref()) {
            (Ok(block), _) => Ok(block.block_header_without_hash.parent_hash),
            (Err(StateSyncError::BlockNotFound(_)), Some(starknet_client)) => {
                // As a fallback, try to get the block hash through the feeder directly. This
                // method is faster than get_block which the sync runner uses.
                // TODO(shahak): Test this flow.
                starknet_client
                    .block_hash(block_number)
                    .await?
                    .ok_or(StateSyncError::BlockNotFound(block_number))
            }
            (Err(err), _) => Err(err),
        }
>>>>>>> origin/main-v0.14.0
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

    fn get_latest_block_number(&self) -> StateSyncResult<Option<BlockNumber>> {
        let txn = self.storage_reader.begin_ro_txn()?;
        let latest_block_number = latest_synced_block(&txn)?;
        Ok(latest_block_number)
    }

    fn is_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncResult<bool> {
        let class_definition_block_number_opt = self
            .storage_reader
            .begin_ro_txn()?
            .get_state_reader()?
            .get_class_definition_block_number(&class_hash)?;
        if let Some(class_definition_block_number) = class_definition_block_number_opt {
            return Ok(class_definition_block_number <= block_number);
        }

        // TODO(noamsp): Add unit testing for cairo0
        let deprecated_class_definition_block_number_opt = self
            .storage_reader
            .begin_ro_txn()?
            .get_state_reader()?
            .get_deprecated_class_definition_block_number(&class_hash)?;

        Ok(deprecated_class_definition_block_number_opt.is_some_and(
            |deprecated_class_definition_block_number| {
                deprecated_class_definition_block_number <= block_number
            },
        ))
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
    ConcurrentLocalComponentServer<StateSync, StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncServer = RemoteComponentServer<StateSyncRequest, StateSyncResponse>;

impl ComponentStarter for StateSync {}
