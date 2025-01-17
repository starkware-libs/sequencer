use std::collections::HashSet;
use std::mem;
use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::{
    Event,
    L1ProviderClient,
    L1ProviderClientResult,
    ValidationStatus,
};

use crate::l1_provider::L1Provider;
use crate::soft_delete_index_map::SoftDeleteIndexMap;
use crate::transaction_manager::TransactionManager;
use crate::ProviderState;

// Represents the internal content of the L1 provider for testing.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
pub struct L1ProviderContent {
    tx_manager_content: Option<TransactionManagerContent>,
    state: Option<ProviderState>,
    current_height: BlockNumber,
}

impl From<L1ProviderContent> for L1Provider {
    fn from(content: L1ProviderContent) -> L1Provider {
        L1Provider {
            tx_manager: content.tx_manager_content.map(Into::into).unwrap_or_default(),
            state: content.state.unwrap_or_default(),
            current_height: content.current_height,
        }
    }
}

#[derive(Debug, Default)]
pub struct L1ProviderContentBuilder {
    tx_manager_content_builder: TransactionManagerContentBuilder,
    state: Option<ProviderState>,
}

impl L1ProviderContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(mut self, state: ProviderState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_txs(txs);
        self
    }

    pub fn with_committed(mut self, tx_hashes: impl IntoIterator<Item = TransactionHash>) -> Self {
        self.tx_manager_content_builder = self.tx_manager_content_builder.with_committed(tx_hashes);
        self
    }

    pub fn build(self) -> L1ProviderContent {
        L1ProviderContent {
            tx_manager_content: self.tx_manager_content_builder.build(),
            state: self.state,
            ..Default::default()
        }
    }

    pub fn build_into_l1_provider(self) -> L1Provider {
        self.build().into()
    }
}

// Represents the internal content of the TransactionManager.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
struct TransactionManagerContent {
    txs: Option<Vec<L1HandlerTransaction>>,
    committed: Option<HashSet<TransactionHash>>,
}

impl From<TransactionManagerContent> for TransactionManager {
    fn from(mut content: TransactionManagerContent) -> TransactionManager {
        let txs: Vec<_> = mem::take(&mut content.txs).unwrap();
        TransactionManager {
            txs: SoftDeleteIndexMap::from(txs),
            committed: content
                .committed
                .unwrap_or_default()
                .into_iter()
                .map(|tx_hash| (tx_hash, None))
                .collect(),
        }
    }
}

#[derive(Debug, Default)]
struct TransactionManagerContentBuilder {
    txs: Option<Vec<L1HandlerTransaction>>,
    committed: Option<HashSet<TransactionHash>>,
}

impl TransactionManagerContentBuilder {
    fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.txs = Some(txs.into_iter().collect());
        self
    }

    fn with_committed(mut self, tx_hashes: impl IntoIterator<Item = TransactionHash>) -> Self {
        self.committed = Some(tx_hashes.into_iter().collect());
        self
    }

    fn build(self) -> Option<TransactionManagerContent> {
        if self.is_default() {
            return None;
        }

        Some(TransactionManagerContent { txs: self.txs, committed: self.committed })
    }

    fn is_default(&self) -> bool {
        self.txs.is_none() && self.committed.is_none()
    }
}

#[derive(Default)]
pub struct FakeL1ProviderClient {
    // Interior mutability needed since this is modifying during client API calls, which are all
    // immutable.
    pub events_received: Mutex<Vec<Event>>,
}

impl FakeL1ProviderClient {
    #[track_caller]
    pub fn assert_add_events_received_with(&self, expected: &[Event]) {
        let events_received = mem::take(&mut *self.events_received.lock().unwrap());
        assert_eq!(events_received, expected);
    }
}

#[async_trait]
impl L1ProviderClient for FakeL1ProviderClient {
    async fn get_txs(
        &self,
        _n_txs: usize,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<Vec<ExecutableL1HandlerTransaction>> {
        todo!()
    }

    async fn add_events(&self, events: Vec<Event>) -> L1ProviderClientResult<()> {
        self.events_received.lock().unwrap().extend(events);
        Ok(())
    }

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus> {
        todo!()
    }
}
