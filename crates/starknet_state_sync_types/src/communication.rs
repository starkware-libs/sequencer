use std::sync::Arc;

use async_trait::async_trait;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_sequencer_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_sequencer_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
};
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::errors::StateSyncError;
use crate::state_sync_types::SyncBlock;

#[async_trait]
pub trait StateSyncClient: Send + Sync {
    /// Request for a block at a specific height.
    /// If the block doesn't exist, or if the sync didn't download it yet, returns None.
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>>;

    // Add a new block to the sync storage from another component within the same node.
    async fn add_new_block(
        &self,
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> StateSyncClientResult<()>;

    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt>;

    // TODO: Add get_nonce_at for StateSyncReader
    // TODO: Add get_compiled_class for StateSyncReader
    // TODO: Add get_class_hash_at for StateSyncReader
    // TODO: Add get_compiled_class_hash for StateSyncReader
    // TODO: Add get_block_info for StateSyncReader
}

pub type StateSyncResult<T> = Result<T, StateSyncError>;

#[derive(Clone, Debug, Error)]
pub enum StateSyncClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    StateSyncError(#[from] StateSyncError),
}
pub type StateSyncClientResult<T> = Result<T, StateSyncClientError>;

pub type LocalStateSyncClient = LocalComponentClient<StateSyncRequest, StateSyncResponse>;
pub type RemoteStateSyncClient = RemoteComponentClient<StateSyncRequest, StateSyncResponse>;
pub type SharedStateSyncClient = Arc<dyn StateSyncClient>;
pub type StateSyncRequestAndResponseSender =
    ComponentRequestAndResponseSender<StateSyncRequest, StateSyncResponse>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncRequest {
    GetBlock(BlockNumber),
    AddNewBlock(BlockNumber, SyncBlock),
    GetStorageAt(BlockNumber, ContractAddress, StorageKey),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncResponse {
    GetBlock(StateSyncResult<Option<SyncBlock>>),
    AddNewBlock(StateSyncResult<()>),
    GetStorageAt(StateSyncResult<Felt>),
}

#[async_trait]
impl StateSyncClient for LocalStateSyncClient {
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>> {
        let request = StateSyncRequest::GetBlock(block_number);
        let response = self.send(request).await;
        handle_response_variants!(StateSyncResponse, GetBlock, StateSyncClientError, StateSyncError)
    }

    async fn add_new_block(
        &self,
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> StateSyncClientResult<()> {
        let request = StateSyncRequest::AddNewBlock(block_number, sync_block);
        let response = self.send(request).await;
        handle_response_variants!(
            StateSyncResponse,
            AddNewBlock,
            StateSyncClientError,
            StateSyncError
        )
    }

    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt> {
        let request = StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key);
        let response = self.send(request).await;
        handle_response_variants!(
            StateSyncResponse,
            GetStorageAt,
            StateSyncClientError,
            StateSyncError
        )
    }
}

#[async_trait]
impl StateSyncClient for RemoteStateSyncClient {
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>> {
        let request = StateSyncRequest::GetBlock(block_number);
        let response = self.send(request).await;
        handle_response_variants!(StateSyncResponse, GetBlock, StateSyncClientError, StateSyncError)
    }

    async fn add_new_block(
        &self,
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> StateSyncClientResult<()> {
        let request = StateSyncRequest::AddNewBlock(block_number, sync_block);
        let response = self.send(request).await;
        handle_response_variants!(
            StateSyncResponse,
            AddNewBlock,
            StateSyncClientError,
            StateSyncError
        )
    }

    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt> {
        let request = StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key);
        let response = self.send(request).await;
        handle_response_variants!(
            StateSyncResponse,
            GetStorageAt,
            StateSyncClientError,
            StateSyncError
        )
    }
}
