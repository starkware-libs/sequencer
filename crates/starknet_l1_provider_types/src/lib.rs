pub mod errors;

use std::sync::Arc;

use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::L1Event;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use tracing::instrument;

use crate::errors::{L1ProviderClientError, L1ProviderError};

pub type L1ProviderResult<T> = Result<T, L1ProviderError>;
pub type L1ProviderClientResult<T> = Result<T, L1ProviderClientError>;
pub type SharedL1ProviderClient = Arc<dyn L1ProviderClient>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    AlreadyIncludedInProposedBlock,
    AlreadyIncludedOnL2,
    ConsumedOnL1OrUnknown,
    Validated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1ProviderRequest {
    AddEvents(Vec<Event>),
    CommitBlock { l1_handler_tx_hashes: Vec<TransactionHash>, height: BlockNumber },
    GetTransactions { n_txs: usize, height: BlockNumber },
    Initialize(Vec<Event>),
    StartBlock { state: SessionState, height: BlockNumber },
    Validate { tx_hash: TransactionHash, height: BlockNumber },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1ProviderResponse {
    AddEvents(L1ProviderResult<()>),
    CommitBlock(L1ProviderResult<()>),
    GetTransactions(L1ProviderResult<Vec<L1HandlerTransaction>>),
    Initialize(L1ProviderResult<()>),
    StartBlock(L1ProviderResult<()>),
    Validate(L1ProviderResult<ValidationStatus>),
}

/// Serves as the provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait L1ProviderClient: Send + Sync {
    async fn start_block(
        &self,
        state: SessionState,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()>;

    async fn get_txs(
        &self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1ProviderClientResult<Vec<L1HandlerTransaction>>;

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus>;

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: Vec<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()>;

    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()>;
    async fn initialize(&self, events: Vec<Event>) -> L1ProviderClientResult<()>;
}

#[async_trait]
impl<ComponentClientType> L1ProviderClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<L1ProviderRequest, L1ProviderResponse>,
{
    #[instrument(skip(self))]
    async fn start_block(
        &self,
        state: SessionState,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        let request = L1ProviderRequest::StartBlock { state, height };
        handle_all_response_variants!(
            L1ProviderResponse,
            StartBlock,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }

    #[instrument(skip(self))]
    async fn get_txs(
        &self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1ProviderClientResult<Vec<L1HandlerTransaction>> {
        let request = L1ProviderRequest::GetTransactions { n_txs, height };
        handle_all_response_variants!(
            L1ProviderResponse,
            GetTransactions,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }

    async fn validate(
        &self,
        tx_hash: TransactionHash,
        height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus> {
        let request = L1ProviderRequest::Validate { tx_hash, height };
        handle_all_response_variants!(
            L1ProviderResponse,
            Validate,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: Vec<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        let request = L1ProviderRequest::CommitBlock { l1_handler_tx_hashes, height };
        handle_all_response_variants!(
            L1ProviderResponse,
            CommitBlock,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }

    #[instrument(skip(self))]
    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()> {
        let request = L1ProviderRequest::AddEvents(events);
        handle_all_response_variants!(
            L1ProviderResponse,
            AddEvents,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }

    async fn initialize(&self, events: Vec<Event>) -> L1ProviderClientResult<()> {
        let request = L1ProviderRequest::Initialize(events);
        handle_all_response_variants!(
            L1ProviderResponse,
            Initialize,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Event {
    L1HandlerTransaction(L1HandlerTransaction),
    TransactionCanceled(L1Event),
    TransactionCancellationStarted(L1Event),
    TransactionConsumed(L1Event),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Propose,
    Validate,
}
