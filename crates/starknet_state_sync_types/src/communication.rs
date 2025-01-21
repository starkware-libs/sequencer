use std::sync::Arc;

use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
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
use crate::state_sync_types::{StateSyncResult, SyncBlock};

#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait StateSyncClient: Send + Sync {
    /// Request for a block at a specific height.
    /// Returns None if the block doesn't exist or the sync hasn't downloaded it yet.
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>>;

    /// Notify the sync that a new block has been created within the node so that other peers can
    /// learn about it through sync.
    async fn add_new_block(&self, sync_block: SyncBlock) -> StateSyncClientResult<()>;

    /// Request storage value under the given key in the given contract instance.
    /// Returns a [BlockNotFound](StateSyncError::BlockNotFound) error if the block doesn't exist or
    /// the sync hasn't been downloaded yet.
    /// Returns a [ContractNotFound](StateSyncError::ContractNotFound) error If the contract has not
    /// been deployed.
    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt>;

    /// Request nonce in the given contract instance.
    /// Returns a [BlockNotFound](StateSyncError::BlockNotFound) error if the block doesn't exist or
    /// the sync hasn't been downloaded yet.
    /// Returns a [ContractNotFound](StateSyncError::ContractNotFound) error If the contract has not
    /// been deployed.
    async fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<Nonce>;

    /// Request class hash of contract class in the given contract instance.
    /// Returns a [BlockNotFound](StateSyncError::BlockNotFound) error if the block doesn't exist or
    /// the sync hasn't been downloaded yet.
    /// Returns a [ContractNotFound](StateSyncError::ContractNotFound) error If the contract has not
    /// been deployed.
    async fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<ClassHash>;

    // TODO(Shahak): Remove this and fix sync state reader once the compiler component is ready.
    async fn get_compiled_class_deprecated(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<ContractClass>;

    /// Request latest block number the sync has downloaded.
    /// Returns None if no latest block was yet downloaded.
    async fn get_latest_block_number(&self) -> StateSyncClientResult<Option<BlockNumber>>;

    // TODO(Shahak): Add get_compiled_class_hash for StateSyncReader
}

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
    AddNewBlock(Box<SyncBlock>),
    GetStorageAt(BlockNumber, ContractAddress, StorageKey),
    GetNonceAt(BlockNumber, ContractAddress),
    GetClassHashAt(BlockNumber, ContractAddress),
    GetCompiledClassDeprecated(BlockNumber, ClassHash),
    GetLatestBlockNumber(),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StateSyncResponse {
    GetBlock(StateSyncResult<Box<Option<SyncBlock>>>),
    AddNewBlock(StateSyncResult<()>),
    GetStorageAt(StateSyncResult<Felt>),
    GetNonceAt(StateSyncResult<Nonce>),
    GetClassHashAt(StateSyncResult<ClassHash>),
    GetCompiledClassDeprecated(StateSyncResult<ContractClass>),
    GetLatestBlockNumber(StateSyncResult<Option<BlockNumber>>),
}

#[async_trait]
impl<ComponentClientType> StateSyncClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<StateSyncRequest, StateSyncResponse>,
{
    async fn get_block(
        &self,
        block_number: BlockNumber,
    ) -> StateSyncClientResult<Option<SyncBlock>> {
        let request = StateSyncRequest::GetBlock(block_number);
        handle_all_response_variants!(
            StateSyncResponse,
            GetBlock,
            StateSyncClientError,
            StateSyncError,
            Boxed
        )
    }

    async fn add_new_block(&self, sync_block: SyncBlock) -> StateSyncClientResult<()> {
        let request = StateSyncRequest::AddNewBlock(Box::new(sync_block));
        handle_all_response_variants!(
            StateSyncResponse,
            AddNewBlock,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }

    async fn get_storage_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
        storage_key: StorageKey,
    ) -> StateSyncClientResult<Felt> {
        let request = StateSyncRequest::GetStorageAt(block_number, contract_address, storage_key);
        handle_all_response_variants!(
            StateSyncResponse,
            GetStorageAt,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }

    async fn get_nonce_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<Nonce> {
        let request = StateSyncRequest::GetNonceAt(block_number, contract_address);
        handle_all_response_variants!(
            StateSyncResponse,
            GetNonceAt,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }

    async fn get_class_hash_at(
        &self,
        block_number: BlockNumber,
        contract_address: ContractAddress,
    ) -> StateSyncClientResult<ClassHash> {
        let request = StateSyncRequest::GetClassHashAt(block_number, contract_address);
        handle_all_response_variants!(
            StateSyncResponse,
            GetClassHashAt,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }

    async fn get_compiled_class_deprecated(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<ContractClass> {
        let request = StateSyncRequest::GetCompiledClassDeprecated(block_number, class_hash);
        handle_all_response_variants!(
            StateSyncResponse,
            GetCompiledClassDeprecated,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }

    async fn get_latest_block_number(&self) -> StateSyncClientResult<Option<BlockNumber>> {
        let request = StateSyncRequest::GetLatestBlockNumber();
        handle_all_response_variants!(
            StateSyncResponse,
            GetLatestBlockNumber,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }
}
