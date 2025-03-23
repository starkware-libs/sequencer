use std::collections::HashSet;
use std::mem;
use std::sync::Mutex;

use async_trait::async_trait;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::{
    L1HandlerTransaction as ExecutableL1HandlerTransaction,
    L1HandlerTransaction,
};
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::{
    Event,
    L1ProviderClient,
    L1ProviderClientResult,
    SessionState,
    ValidationStatus,
};

use crate::bootstrapper::CommitBlockBacklog;
use crate::l1_provider::L1Provider;
use crate::soft_delete_index_map::SoftDeleteIndexMap;
use crate::transaction_manager::TransactionManager;
use crate::ProviderState;

pub fn l1_handler(tx_hash: usize) -> L1HandlerTransaction {
    let tx_hash = TransactionHash(StarkHash::from(tx_hash));
    executable_l1_handler_tx(L1HandlerTxArgs { tx_hash, ..Default::default() })
}

// Represents the internal content of the L1 provider for testing.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Default)]
pub struct L1ProviderContent {
    tx_manager_content: Option<TransactionManagerContent>,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
}

impl L1ProviderContent {
    #[track_caller]
    pub fn assert_eq(&self, l1_provider: &L1Provider) {
        if let Some(tx_manager_content) = &self.tx_manager_content {
            tx_manager_content.assert_eq(&l1_provider.tx_manager);
        }

        if let Some(state) = &self.state {
            assert_eq!(&l1_provider.state, state);
        }
    }
}

impl From<L1ProviderContent> for L1Provider {
    fn from(content: L1ProviderContent) -> L1Provider {
        L1Provider {
            tx_manager: content.tx_manager_content.map(Into::into).unwrap_or_default(),
            // Defaulting to Pending state, since a provider with a "default" Bootstrapper
            // is functionally equivalent to Pending for testing purposes.
            state: content.state.unwrap_or(ProviderState::Pending),
            current_height: content.current_height.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct L1ProviderContentBuilder {
    tx_manager_content_builder: TransactionManagerContentBuilder,
    state: Option<ProviderState>,
    current_height: Option<BlockNumber>,
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

    pub fn with_height(mut self, height: BlockNumber) -> Self {
        self.current_height = Some(height);
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
            current_height: self.current_height,
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
    pub txs: Option<Vec<L1HandlerTransaction>>,
    pub committed: Option<HashSet<TransactionHash>>,
}

impl TransactionManagerContent {
    #[track_caller]
    fn assert_eq(&self, tx_manager: &TransactionManager) {
        if let Some(txs) = &self.txs {
            assert_eq!(
                txs,
                &tx_manager.txs.txs.values().map(|tx| tx.transaction.clone()).collect_vec()
            );
        }

        if let Some(committed) = &self.committed {
            assert_eq!(committed, &tx_manager.committed);
        }
    }
}

impl From<TransactionManagerContent> for TransactionManager {
    fn from(mut content: TransactionManagerContent) -> TransactionManager {
        let txs: Vec<_> = mem::take(&mut content.txs).unwrap_or_default();
        TransactionManager {
            txs: SoftDeleteIndexMap::from(txs),
            committed: content.committed.unwrap_or_default(),
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
    pub commit_blocks_received: Mutex<Vec<CommitBlockBacklog>>,
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
    async fn start_block(
        &self,
        _state: SessionState,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        todo!()
    }

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

    async fn commit_block(
        &self,
        l1_handler_tx_hashes: Vec<TransactionHash>,
        height: BlockNumber,
    ) -> L1ProviderClientResult<()> {
        self.commit_blocks_received
            .lock()
            .unwrap()
            .push(CommitBlockBacklog { height, committed_txs: l1_handler_tx_hashes });
        Ok(())
    }

    async fn validate(
        &self,
        _tx_hash: TransactionHash,
        _height: BlockNumber,
    ) -> L1ProviderClientResult<ValidationStatus> {
        todo!()
    }

    async fn initialize(&self, _events: Vec<Event>) -> L1ProviderClientResult<()> {
        todo!()
    }
}
