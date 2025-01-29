use indexmap::{IndexMap, IndexSet};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::{L1Provider, ProviderState, TransactionManager};

// Represents the internal content of the L1 provider for testing.
// Enables customized (and potentially inconsistent) creation for unit testing.
#[derive(Debug, Default)]
pub struct L1ProviderContent {
    tx_manager_content: Option<TransactionManagerContent>,
    state: Option<ProviderState>,
}

impl From<L1ProviderContent> for L1Provider {
    fn from(content: L1ProviderContent) -> L1Provider {
        L1Provider {
            tx_manager: content
                .tx_manager_content
                .map(|tm_content| tm_content.complete_to_tx_manager())
                .unwrap_or_default(),
            state: content.state.unwrap_or_default(),
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

    pub fn with_on_l2_awaiting_l1_consumption(
        mut self,
        tx_hashes: impl IntoIterator<Item = TransactionHash>,
    ) -> Self {
        self.tx_manager_content_builder =
            self.tx_manager_content_builder.with_on_l2_awaiting_l1_consumption(tx_hashes);
        self
    }

    pub fn build(self) -> L1ProviderContent {
        L1ProviderContent {
            tx_manager_content: self.tx_manager_content_builder.build(),
            state: self.state,
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
    txs: Option<IndexMap<TransactionHash, L1HandlerTransaction>>,
    on_l2_awaiting_l1_consumption: Option<IndexSet<TransactionHash>>,
}

impl TransactionManagerContent {
    fn complete_to_tx_manager(self) -> TransactionManager {
        TransactionManager {
            txs: self.txs.unwrap_or_default(),
            on_l2_awaiting_l1_consumption: self.on_l2_awaiting_l1_consumption.unwrap_or_default(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default)]
struct TransactionManagerContentBuilder {
    txs: Option<IndexMap<TransactionHash, L1HandlerTransaction>>,
    on_l2_awaiting_l1_consumption: Option<IndexSet<TransactionHash>>,
}

impl TransactionManagerContentBuilder {
    fn with_txs(mut self, txs: impl IntoIterator<Item = L1HandlerTransaction>) -> Self {
        self.txs = Some(txs.into_iter().map(|tx| (tx.tx_hash, tx)).collect());
        self
    }

    fn with_on_l2_awaiting_l1_consumption(
        mut self,
        tx_hashes: impl IntoIterator<Item = TransactionHash>,
    ) -> Self {
        self.on_l2_awaiting_l1_consumption = Some(tx_hashes.into_iter().collect());
        self
    }

    fn build(self) -> Option<TransactionManagerContent> {
        if self.is_default() {
            return None;
        }

        Some(TransactionManagerContent {
            txs: self.txs,
            on_l2_awaiting_l1_consumption: self.on_l2_awaiting_l1_consumption,
        })
    }

    fn is_default(&self) -> bool {
        self.txs.is_none() && self.on_l2_awaiting_l1_consumption.is_none()
    }
}
