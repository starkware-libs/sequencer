pub mod errors;

use std::fmt::Display;
use std::sync::Arc;

use apollo_infra::component_definitions::{ComponentClient, PrioritizedRequest};
use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_infra::{
    handle_all_response_variants,
    impl_debug_for_infra_requests_and_responses,
    impl_labeled_request,
};
use apollo_metrics::generate_permutation_labels;
use async_trait::async_trait;
use indexmap::IndexSet;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use papyrus_base_layer::L1Event;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp, UnixTimestamp};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::{TransactionHash, TransactionHasher};
use starknet_api::StarknetApiError;
use strum::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use tracing::instrument;

use crate::errors::{L1EventsProviderClientError, L1EventsProviderError};

pub type L1EventsProviderResult<T> = Result<T, L1EventsProviderError>;
pub type L1EventsProviderClientResult<T> = Result<T, L1EventsProviderClientError>;
pub type SharedL1EventsProviderClient = Arc<dyn L1EventsProviderClient>;

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
    L1EventsProviderError,
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(L1EventsProviderRequestLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum L1EventsProviderRequest {
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
    Initialize {
        historic_l2_height: BlockNumber,
        events: Vec<Event>,
    },
    StartBlock {
        state: SessionState,
        height: BlockNumber,
    },
    Validate {
        tx_hash: TransactionHash,
        height: BlockNumber,
    },
    GetL1EventsProviderSnapshot,
    GetProviderState,
}
impl_debug_for_infra_requests_and_responses!(L1EventsProviderRequest);
impl_labeled_request!(L1EventsProviderRequest, L1EventsProviderRequestLabelValue);
impl PrioritizedRequest for L1EventsProviderRequest {}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum L1EventsProviderResponse {
    AddEvents(L1EventsProviderResult<()>),
    CommitBlock(L1EventsProviderResult<()>),
    GetTransactions(L1EventsProviderResult<Vec<L1HandlerTransaction>>),
    Initialize(L1EventsProviderResult<()>),
    StartBlock(L1EventsProviderResult<()>),
    Validate(L1EventsProviderResult<ValidationStatus>),
    GetL1EventsProviderSnapshot(L1EventsProviderResult<L1EventsProviderSnapshot>),
    GetProviderState(L1EventsProviderResult<ProviderState>),
}
impl_debug_for_infra_requests_and_responses!(L1EventsProviderResponse);

/// Serves as the provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait L1EventsProviderClient: Send + Sync {
    async fn start_block(
        &self,
        state: SessionState,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<()>;

    async fn get_txs(
        &self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<Vec<L1HandlerTransaction>>;

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1EventsProviderClientResult<ValidationStatus>;

    async fn commit_block(
        &self,
        l1_handler_consumed_tx_hashes: IndexSet<TransactionHash>,
        l1_handler_rejected_tx_hashes: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<()>;

    async fn add_events(&self, events: Vec<Event>) -> L1EventsProviderClientResult<()>;
    async fn initialize(
        &self,
        historic_l2_height: BlockNumber,
        events: Vec<Event>,
    ) -> L1EventsProviderClientResult<()>;
    async fn get_l1_events_provider_snapshot(
        &self,
    ) -> L1EventsProviderClientResult<L1EventsProviderSnapshot>;
    async fn get_provider_state(&self) -> L1EventsProviderClientResult<ProviderState>;
}

#[async_trait]
impl<ComponentClientType> L1EventsProviderClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<L1EventsProviderRequest, L1EventsProviderResponse>,
{
    #[instrument(skip(self))]
    async fn start_block(
        &self,
        state: SessionState,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<()> {
        let request = L1EventsProviderRequest::StartBlock { state, height };
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            StartBlock,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    #[instrument(skip(self))]
    async fn get_txs(
        &self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<Vec<L1HandlerTransaction>> {
        let request = L1EventsProviderRequest::GetTransactions { n_txs, height };
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            GetTransactions,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    async fn validate(
        &self,
        tx_hash: TransactionHash,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<ValidationStatus> {
        let request = L1EventsProviderRequest::Validate { tx_hash, height };
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            Validate,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: IndexSet<TransactionHash>,
        rejected_tx_hashes: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1EventsProviderClientResult<()> {
        let request = L1EventsProviderRequest::CommitBlock {
            l1_handler_tx_hashes,
            rejected_tx_hashes,
            height,
        };
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            CommitBlock,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    #[instrument(skip(self))]
    async fn add_events(&self, events: Vec<Event>) -> L1EventsProviderClientResult<()> {
        let request = L1EventsProviderRequest::AddEvents(events);
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            AddEvents,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    async fn initialize(
        &self,
        historic_l2_height: BlockNumber,
        events: Vec<Event>,
    ) -> L1EventsProviderClientResult<()> {
        let request = L1EventsProviderRequest::Initialize { historic_l2_height, events };
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            Initialize,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    async fn get_l1_events_provider_snapshot(
        &self,
    ) -> L1EventsProviderClientResult<L1EventsProviderSnapshot> {
        let request = L1EventsProviderRequest::GetL1EventsProviderSnapshot;
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            GetL1EventsProviderSnapshot,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }

    async fn get_provider_state(&self) -> L1EventsProviderClientResult<ProviderState> {
        let request = L1EventsProviderRequest::GetProviderState;
        handle_all_response_variants!(
            self,
            request,
            L1EventsProviderResponse,
            GetProviderState,
            L1EventsProviderClientError,
            L1EventsProviderError,
            Direct
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Event {
    L1HandlerTransaction {
        l1_handler_tx: L1HandlerTransaction,
        block_timestamp: BlockTimestamp,
        scrape_timestamp: UnixTimestamp,
    },
    TransactionCanceled {
        tx_hash: TransactionHash,
    },
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
    pub fn from_l1_event(
        chain_id: &ChainId,
        l1_event: L1Event,
        scrape_timestamp: UnixTimestamp,
    ) -> Result<Self, StarknetApiError> {
        Ok(match l1_event {
            L1Event::LogMessageToL2 { tx, fee, block_timestamp, .. } => {
                let tx = L1HandlerTransaction::create(tx, chain_id, fee)?;
                Self::L1HandlerTransaction { l1_handler_tx: tx, block_timestamp, scrape_timestamp }
            }
            L1Event::MessageToL2CancellationStarted {
                cancelled_tx,
                cancellation_request_timestamp,
            } => {
                let tx_hash =
                    cancelled_tx.calculate_transaction_hash(chain_id, &cancelled_tx.version)?;

                Self::TransactionCancellationStarted { tx_hash, cancellation_request_timestamp }
            }
            L1Event::MessageToL2Canceled { cancelled_tx } => {
                let tx_hash =
                    cancelled_tx.calculate_transaction_hash(chain_id, &cancelled_tx.version)?;
                Self::TransactionCanceled { tx_hash }
            }

            L1Event::ConsumedMessageToL2 { tx, timestamp } => {
                let tx_hash = tx.calculate_transaction_hash(
                    chain_id,
                    &starknet_api::transaction::L1HandlerTransaction::VERSION,
                )?;
                Self::TransactionConsumed { tx_hash, timestamp }
            }
        })
    }

    #[cfg(any(feature = "testing", test))]
    /// Checks if two events are almost equal, allowing for a small margin in scrape time.
    pub fn almost_eq(&self, other: &Event) -> bool {
        match (self, other) {
            (
                Event::L1HandlerTransaction {
                    l1_handler_tx: self_l1_handler_tx,
                    block_timestamp: self_block_timestamp,
                    scrape_timestamp: self_scrape_timestamp,
                },
                Event::L1HandlerTransaction {
                    l1_handler_tx: other_l1_handler_tx,
                    block_timestamp: other_block_timestamp,
                    scrape_timestamp: other_scrape_timestamp,
                },
            ) => {
                const SCRAPE_TIMESTAMP_MARGIN: u64 = 5;

                self_l1_handler_tx == other_l1_handler_tx
                    && self_block_timestamp == other_block_timestamp
                    && self_scrape_timestamp.abs_diff(*other_scrape_timestamp)
                        <= SCRAPE_TIMESTAMP_MARGIN
            }
            _ => self == other,
        }
    }

    #[cfg(any(feature = "testing", test))]
    /// Asserts event matches other, allowing for a small margin in scrape time.
    pub fn assert_event_almost_eq(&self, other: &Event) {
        assert!(self.almost_eq(other), "Event mismatch: {self:?} != {other:?}");
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::L1HandlerTransaction {
                l1_handler_tx: tx,
                block_timestamp,
                scrape_timestamp,
            } => {
                write!(
                    f,
                    "L1HandlerTransaction(tx_hash={}, block_timestamp={}, scrape_timestamp={})",
                    tx.tx_hash, block_timestamp, scrape_timestamp
                )
            }
            Event::TransactionCanceled { tx_hash } => write!(f, "TransactionCanceled({tx_hash})"),
            Event::TransactionCancellationStarted { tx_hash, cancellation_request_timestamp } => {
                write!(
                    f,
                    "TransactionCancellationStarted(tx_hash={tx_hash}, \
                     cancellation_request_block_timestamp={cancellation_request_timestamp})"
                )
            }
            Event::TransactionConsumed { tx_hash, timestamp } => {
                write!(f, "TransactionConsumed(tx_hash={tx_hash}, block_timestamp={timestamp})")
            }
        }
    }
}

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProviderState {
    /// Provider has not been initialized yet, needs to get start_height and probably also catch
    /// up.
    Uninitialized,
    /// Provider is catching up using sync.
    CatchingUp,
    /// Provider is not ready for proposing or validating. Use start_block to transition to Propose
    /// or Validate.
    Pending,
    /// Provider is ready for proposing. Use get_txs to get what you need for a new proposal. Use
    /// commit_block to finish and return to Pending.
    Propose,
    /// Provider is ready for validating. Use validate to validate a transaction. Use commit_block
    /// to finish and return to Pending.
    Validate,
}

impl ProviderState {
    pub fn is_uninitialized(&self) -> bool {
        *self == ProviderState::Uninitialized
    }

    pub fn is_catching_up(&self) -> bool {
        *self == ProviderState::CatchingUp
    }

    pub fn transition_to_pending(&self) -> ProviderState {
        assert!(
            !self.is_catching_up(),
            "Transitioning from catching up should be done manually by the L1EventsProvider."
        );
        ProviderState::Pending
    }
}

impl From<SessionState> for ProviderState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Propose => ProviderState::Propose,
            SessionState::Validate => ProviderState::Validate,
        }
    }
}

impl std::fmt::Display for ProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Propose,
    Validate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct L1EventsProviderSnapshot {
    pub uncommitted_transactions: Vec<TransactionHash>,
    pub uncommitted_staged_transactions: Vec<TransactionHash>,
    pub rejected_transactions: Vec<TransactionHash>,
    pub rejected_staged_transactions: Vec<TransactionHash>,
    pub committed_transactions: Vec<TransactionHash>,
    pub cancellation_started_on_l2: Vec<TransactionHash>,
    pub cancelled_on_l2: Vec<TransactionHash>,
    pub consumed: Vec<TransactionHash>,
    pub l1_events_provider_state: String,
    pub current_height: BlockNumber,
    pub number_of_txs_in_records: usize,
}

generate_permutation_labels! {
    L1_EVENTS_PROVIDER_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, L1EventsProviderRequestLabelValue),
}
