pub mod errors;

use std::fmt::Display;
use std::sync::Arc;

use apollo_infra::component_client::ClientError;
use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
use indexmap::IndexSet;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::{EventData, L1Event};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp, UnixTimestamp};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::transaction::{TransactionHash, TransactionHasher};
use starknet_api::StarknetApiError;
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
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

impl From<InvalidValidationStatus> for ValidationStatus {
    fn from(status: InvalidValidationStatus) -> Self {
        Self::Invalid(status)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvalidValidationStatus {
    AlreadyIncludedInProposedBlock,
    AlreadyIncludedOnL2,
    CancelledOnL2,
    // This tx can be safely deleted from the records.
    ConsumedOnL1,
    // This tx is either never been seen or was seen, consumed, and deleted.
    NotFound,
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(L1ProviderRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum L1ProviderRequest {
    AddEvents(Vec<Event>),
    CommitBlock {
        l1_handler_tx_hashes: IndexSet<TransactionHash>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
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
    GetL1ProviderSnapshot,
}
impl_debug_for_infra_requests_and_responses!(L1ProviderRequest);
impl_labeled_request!(L1ProviderRequest, L1ProviderRequestLabelValue);
impl PrioritizedRequest for L1ProviderRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1ProviderResponse {
    AddEvents(L1ProviderResult<()>),
    CommitBlock(L1ProviderResult<()>),
    GetTransactions(L1ProviderResult<Vec<L1HandlerTransaction>>),
    Initialize(L1ProviderResult<()>),
    StartBlock(L1ProviderResult<()>),
    Validate(L1ProviderResult<ValidationStatus>),
    GetL1ProviderSnapshot(L1ProviderResult<L1ProviderSnapshot>),
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
        l1_handler_consumed_tx_hashes: IndexSet<TransactionHash>,
        l1_handler_rejected_tx_hashes: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()>;

    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()>;
    async fn initialize(&self, events: Vec<Event>) -> L1ProviderClientResult<()>;
    async fn get_l1_provider_snapshot(&self) -> L1ProviderClientResult<L1ProviderSnapshot>;
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
        l1_handler_tx_hashes: IndexSet<TransactionHash>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
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

    async fn get_l1_provider_snapshot(&self) -> L1ProviderClientResult<L1ProviderSnapshot> {
        let request = L1ProviderRequest::GetL1ProviderSnapshot;
        handle_all_response_variants!(
            L1ProviderResponse,
            GetL1ProviderSnapshot,
            L1ProviderClientError,
            L1ProviderError,
            Direct
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Event {
    L1HandlerTransaction {
        l1_handler_tx: L1HandlerTransaction,
        block_timestamp: BlockTimestamp,
        log_timestamp: UnixTimestamp,
    },
    TransactionCanceled(EventData),
    TransactionCancellationStarted {
        tx_hash: TransactionHash,
        cancellation_request_timestamp: BlockTimestamp,
    },
    TransactionConsumed {
        tx_hash: TransactionHash,
        timestamp: BlockTimestamp,
    },
}

impl Event {
    pub fn from_l1_event(chain_id: &ChainId, l1_event: L1Event) -> Result<Self, StarknetApiError> {
        Ok(match l1_event {
            L1Event::LogMessageToL2 { tx, fee, block_timestamp, log_timestamp, .. } => {
                let tx = ExecutableL1HandlerTransaction::create(tx, chain_id, fee)?;
                Self::L1HandlerTransaction { l1_handler_tx: tx, block_timestamp, log_timestamp }
            }
            L1Event::MessageToL2CancellationStarted {
                cancelled_tx,
                cancellation_request_timestamp,
            } => {
                let tx_hash =
                    cancelled_tx.calculate_transaction_hash(chain_id, &cancelled_tx.version)?;

                Self::TransactionCancellationStarted { tx_hash, cancellation_request_timestamp }
            }
            L1Event::MessageToL2Canceled(event_data) => Self::TransactionCanceled(event_data),
            L1Event::ConsumedMessageToL2 { tx, timestamp } => {
                let tx_hash = tx.calculate_transaction_hash(
                    chain_id,
                    &starknet_api::transaction::L1HandlerTransaction::VERSION,
                )?;
                Self::TransactionConsumed { tx_hash, timestamp }
            }
        })
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::L1HandlerTransaction { l1_handler_tx: tx, block_timestamp, log_timestamp } => {
                write!(
                    f,
                    "L1HandlerTransaction(tx_hash={}, block_timestamp={}, log_timestamp={})",
                    tx.tx_hash, block_timestamp, log_timestamp
                )
            }
            Event::TransactionCanceled(data) => write!(f, "TransactionCanceled({})", data),
            Event::TransactionCancellationStarted { tx_hash, cancellation_request_timestamp } => {
                write!(
                    f,
                    "TransactionCancellationStarted(tx_hash={}, \
                     cancellation_request_block_timestamp={})",
                    tx_hash, cancellation_request_timestamp
                )
            }
            Event::TransactionConsumed { tx_hash, timestamp } => {
                write!(f, "TransactionConsumed(tx_hash={}, block_timestamp={})", tx_hash, timestamp)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Propose,
    Validate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct L1ProviderSnapshot {
    pub uncommitted_transactions: Vec<TransactionHash>,
    pub uncommitted_staged_transactions: Vec<TransactionHash>,
    pub rejected_transactions: Vec<TransactionHash>,
    pub rejected_staged_transactions: Vec<TransactionHash>,
    pub committed_transactions: Vec<TransactionHash>,
    pub l1_provider_state: String,
    pub current_height: BlockNumber,
}
