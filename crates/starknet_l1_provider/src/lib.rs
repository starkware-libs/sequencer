pub mod communication;

pub mod l1_provider;
pub mod l1_scraper;
#[cfg(test)]
pub mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use indexmap::{IndexMap, IndexSet};
use papyrus_base_layer::constants::{
    EventIdentifier,
    CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
    LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
};
use papyrus_config::converters::deserialize_milliseconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::{L1ProviderResult, ValidationStatus};
use validator::Validate;

#[cfg(test)]
#[path = "l1_provider_tests.rs"]
pub mod l1_provider_tests;

#[derive(Debug, Default)]
struct TransactionManager {
    txs: IndexMap<TransactionHash, L1HandlerTransaction>,
    proposed_txs: IndexSet<TransactionHash>,
    on_l2_awaiting_l1_consumption: IndexSet<TransactionHash>,
}

impl TransactionManager {
    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let (tx_hashes, txs): (Vec<_>, Vec<_>) = self
            .txs
            .iter()
            .skip(self.proposed_txs.len()) // Transactions are proposed FIFO.
            .take(n_txs)
            .map(|(&hash, tx)| (hash, tx.clone()))
            .unzip();

        self.proposed_txs.extend(tx_hashes);
        txs
    }

    pub fn tx_status(&self, tx_hash: TransactionHash) -> ValidationStatus {
        if self.txs.contains_key(&tx_hash) {
            ValidationStatus::Validated
        } else if self.on_l2_awaiting_l1_consumption.contains(&tx_hash) {
            ValidationStatus::AlreadyIncludedOnL2
        } else {
            ValidationStatus::ConsumedOnL1OrUnknown
        }
    }

    pub fn _add_unconsumed_l1_not_in_l2_block_tx(&mut self, _tx: L1HandlerTransaction) {
        todo!(
            "Check if tx is in L2, if it isn't on L2 add it to the txs buffer, otherwise print
             debug and do nothing."
        )
    }

    pub fn _mark_tx_included_on_l2(&mut self, _tx_hash: &TransactionHash) {
        todo!("Adds the tx hash to l2 buffer; remove tx from the txs storage if it's there.")
    }
}

/// Current state of the provider, where pending means: idle, between proposal/validation cycles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProviderState {
    Pending,
    Propose,
    #[default]
    Uninitialized,
    Validate,
}

impl ProviderState {
    fn transition_to_propose(self) -> L1ProviderResult<Self> {
        match self {
            ProviderState::Pending => Ok(ProviderState::Propose),
            _ => Err(L1ProviderError::unexpected_transition(self, ProviderState::Propose)),
        }
    }

    fn transition_to_validate(self) -> L1ProviderResult<Self> {
        match self {
            ProviderState::Pending => Ok(ProviderState::Validate),
            _ => Err(L1ProviderError::unexpected_transition(self, ProviderState::Validate)),
        }
    }

    fn _transition_to_pending(self) -> L1ProviderResult<Self> {
        todo!()
    }

    pub fn as_str(&self) -> &str {
        match self {
            ProviderState::Pending => "Pending",
            ProviderState::Propose => "Propose",
            ProviderState::Uninitialized => "Uninitialized",
            ProviderState::Validate => "Validate",
        }
    }
}

impl std::fmt::Display for ProviderState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1ProviderConfig {
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub _poll_interval: Duration,
}

impl SerializeConfig for L1ProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "_poll_interval",
            &Duration::from_millis(100).as_millis(),
            "Interval in milliseconds between each scraping attempt of L1.",
            ParamPrivacyInput::Public,
        )])
    }
}

pub const fn event_identifiers_to_track() -> &'static [EventIdentifier] {
    &[
        LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER,
        CONSUMED_MESSAGE_TO_L1_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER,
        MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER,
    ]
}
