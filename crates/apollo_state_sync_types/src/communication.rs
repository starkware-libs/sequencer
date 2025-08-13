use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    PrioritizedRequest,
    RequestPriority,
    RequestWrapper,
};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

use crate::errors::StateSyncError;
use crate::state_sync_types::{StateSyncResult, SyncBlock};

#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait StateSyncClient: Send + Sync {
    /// Request for a block at a specific height.
    /// Returns a [BlockNotFound](StateSyncError::BlockNotFound) error if the block doesn't exist or
    /// the sync hasn't been downloaded yet.
    async fn get_block(&self, block_number: BlockNumber) -> StateSyncClientResult<SyncBlock>;

    /// Request for a block hash at a specific height.
    /// Returns a [BlockNotFound](StateSyncError::BlockNotFound) error if the block doesn't exist or
    /// the sync hasn't been downloaded yet.
    async fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncClientResult<BlockHash>;

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

    /// Request latest block number the sync has downloaded.
    /// Returns None if no latest block was yet downloaded.
    async fn get_latest_block_number(&self) -> StateSyncClientResult<Option<BlockNumber>>;

    /// Returns whether the given class was declared at the given block or before it.
    async fn is_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<bool>;

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
pub type StateSyncRequestWrapper = RequestWrapper<StateSyncRequest, StateSyncResponse>;

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(StateSyncRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum StateSyncRequest {
    GetBlock(BlockNumber),
    GetBlockHash(BlockNumber),
    AddNewBlock(Box<SyncBlock>),
    GetStorageAt(BlockNumber, ContractAddress, StorageKey),
    GetNonceAt(BlockNumber, ContractAddress),
    GetClassHashAt(BlockNumber, ContractAddress),
    GetLatestBlockNumber(),
    IsClassDeclaredAt(BlockNumber, ClassHash),
}
impl_debug_for_infra_requests_and_responses!(StateSyncRequest);
impl_labeled_request!(StateSyncRequest, StateSyncRequestLabelValue);
impl PrioritizedRequest for StateSyncRequest {
    fn priority(&self) -> RequestPriority {
        match self {
            StateSyncRequest::GetBlock(_) | StateSyncRequest::GetBlockHash(_) => {
                RequestPriority::High
            }
            StateSyncRequest::GetStorageAt(_, _, _)
            | StateSyncRequest::GetNonceAt(_, _)
            | StateSyncRequest::GetClassHashAt(_, _)
            | StateSyncRequest::AddNewBlock(_)
            | StateSyncRequest::GetLatestBlockNumber()
            | StateSyncRequest::IsClassDeclaredAt(_, _) => RequestPriority::Normal,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum StateSyncResponse {
    GetBlock(StateSyncResult<Box<SyncBlock>>),
    GetBlockHash(StateSyncResult<BlockHash>),
    AddNewBlock(StateSyncResult<()>),
    GetStorageAt(StateSyncResult<Felt>),
    GetNonceAt(StateSyncResult<Nonce>),
    GetClassHashAt(StateSyncResult<ClassHash>),
    GetLatestBlockNumber(StateSyncResult<Option<BlockNumber>>),
    IsClassDeclaredAt(StateSyncResult<bool>),
}
impl_debug_for_infra_requests_and_responses!(StateSyncResponse);

#[async_trait]
impl<ComponentClientType> StateSyncClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<StateSyncRequest, StateSyncResponse>,
{
    async fn get_block(&self, block_number: BlockNumber) -> StateSyncClientResult<SyncBlock> {
        let request = StateSyncRequest::GetBlock(block_number);
        handle_all_response_variants!(
            StateSyncResponse,
            GetBlock,
            StateSyncClientError,
            StateSyncError,
            Boxed
        )
    }

    async fn get_block_hash(&self, block_number: BlockNumber) -> StateSyncClientResult<BlockHash> {
        let request = StateSyncRequest::GetBlockHash(block_number);
        handle_all_response_variants!(
            StateSyncResponse,
            GetBlockHash,
            StateSyncClientError,
            StateSyncError,
            Direct
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

    async fn is_class_declared_at(
        &self,
        block_number: BlockNumber,
        class_hash: ClassHash,
    ) -> StateSyncClientResult<bool> {
        let request = StateSyncRequest::IsClassDeclaredAt(block_number, class_hash);
        handle_all_response_variants!(
            StateSyncResponse,
            IsClassDeclaredAt,
            StateSyncClientError,
            StateSyncError,
            Direct
        )
    }
}
