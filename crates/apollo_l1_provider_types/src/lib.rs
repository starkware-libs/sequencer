pub mod errors;

use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::ComponentClient;
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::{EventData, L1Event};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::transaction::{TransactionHash, TransactionHasher};
use starknet_api::StarknetApiError;
use strum_macros::AsRefStr;
use tracing::instrument;

use crate::errors::{L1ProviderClientError, L1ProviderError};

pub type L1ProviderResult<T> = Result<T, L1ProviderError>;
pub type L1ProviderClientResult<T> = Result<T, L1ProviderClientError>;
pub type SharedL1ProviderClient = Arc<dyn L1ProviderClient>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    Invalid(InvalidValidationStatus),
    Validated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidValidationStatus {
    AlreadyIncludedInProposedBlock,
    AlreadyIncludedOnL2,
    ConsumedOnL1OrUnknown,
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1ProviderRequest {
    AddEvents(Vec<Event>),
    CommitBlock {
        l1_handler_tx_hashes: Vec<TransactionHash>,
        rejected_tx_hashes: HashSet<TransactionHash>,
        height: BlockNumber,
    },
    GetTransactions {
        n_txs: usize,
        height: BlockNumber,
    },
    Initialize(Vec<Event>),
    StartBlock {
        state: SessionState,
        height: BlockNumber,
    },
    Validate {
        tx_hash: TransactionHash,
        height: BlockNumber,
    },
}
impl_debug_for_infra_requests_and_responses!(L1ProviderRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1ProviderResponse {
    AddEvents(L1ProviderResult<()>),
    CommitBlock(L1ProviderResult<()>),
    GetTransactions(L1ProviderResult<Vec<L1HandlerTransaction>>),
    Initialize(L1ProviderResult<()>),
    StartBlock(L1ProviderResult<()>),
    Validate(L1ProviderResult<ValidationStatus>),
}
impl_debug_for_infra_requests_and_responses!(L1ProviderResponse);

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
        l1_handler_consumed_tx_hashes: Vec<TransactionHash>,
        l1_handler_rejected_tx_hashes: HashSet<TransactionHash>,
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
        rejected_tx_hashes: HashSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        let request =
            L1ProviderRequest::CommitBlock { l1_handler_tx_hashes, rejected_tx_hashes, height };
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
    TransactionCanceled(EventData),
    TransactionCancellationStarted(TransactionHash),
    TransactionConsumed(EventData),
}

impl Event {
    pub fn from_l1_event(chain_id: &ChainId, l1_event: L1Event) -> Result<Self, StarknetApiError> {
        Ok(match l1_event {
            L1Event::LogMessageToL2 { tx, fee, .. } => {
                let tx = ExecutableL1HandlerTransaction::create(tx, chain_id, fee)?;
                Self::L1HandlerTransaction(tx)
            }
            L1Event::MessageToL2CancellationStarted { cancelled_tx } => {
                let tx_hash =
                    cancelled_tx.calculate_transaction_hash(chain_id, &cancelled_tx.version)?;

                Self::TransactionCancellationStarted(tx_hash)
            }
            L1Event::MessageToL2Canceled(event_data) => Self::TransactionCanceled(event_data),
            L1Event::ConsumedMessageToL2(event_data) => Self::TransactionConsumed(event_data),
        })
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::L1HandlerTransaction(tx) => {
                write!(f, "L1HandlerTransaction(tx_hash={})", tx.tx_hash)
            }
            Event::TransactionCanceled(data) => write!(f, "TransactionCanceled({})", data),
            Event::TransactionCancellationStarted(data) => {
                write!(f, "TransactionCancellationStarted({})", data)
            }
            Event::TransactionConsumed(data) => write!(f, "TransactionConsumed({})", data),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Propose,
    Validate,
}
