pub mod communication;
pub mod errors;
pub mod l1_scraper;

#[cfg(test)]
pub mod test_utils;

use std::collections::BTreeMap;
use std::time::Duration;

use indexmap::{IndexMap, IndexSet};
use papyrus_config::converters::deserialize_milliseconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::errors::L1ProviderError;
use starknet_l1_provider_types::{L1ProviderResult, ValidationStatus};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use validator::Validate;

#[cfg(test)]
#[path = "l1_provider_tests.rs"]
pub mod l1_provider_tests;

// TODO: optimistic proposer support, will add later to keep things simple, but the design here
// is compatible with it.
#[derive(Debug, Default)]
pub struct L1Provider {
    tx_manager: TransactionManager,
    // TODO(Gilad): consider transitioning to a generic phantom state once the infra is stabilized
    // and we see how well it handles consuming the L1Provider when moving between states.
    state: ProviderState,
    current_height: BlockNumber,
}

impl L1Provider {
    pub fn new(_config: L1ProviderConfig) -> L1ProviderResult<Self> {
        todo!("Init crawler in uninitialized_state from config, to initialize call `reset`.");
    }

    /// Retrieves up to `n_txs` transactions that have yet to be proposed or accepted on L2.
    pub fn get_txs(
        &mut self,
        n_txs: usize,
        height: BlockNumber,
    ) -> L1ProviderResult<Vec<L1HandlerTransaction>> {
        // Reenable once `commit_block` is implemented so that height can be updated.
        let _disabled = self.validate_height(height);

        match self.state {
            ProviderState::Propose => Ok(self.tx_manager.get_txs(n_txs)),
            ProviderState::Pending => Err(L1ProviderError::GetTransactionsInPendingState),
            ProviderState::Validate => Err(L1ProviderError::GetTransactionConsensusBug),
            ProviderState::Uninitialized => panic!("Uninitialized L1 provider"),
        }
    }

    /// Returns true if and only if the given transaction is both not included in an L2 block, and
    /// unconsumed on L1.
    pub fn validate(
        &mut self,
        tx_hash: TransactionHash,
        height: BlockNumber,
    ) -> L1ProviderResult<ValidationStatus> {
        self.validate_height(height)?;
        match self.state {
            ProviderState::Validate => Ok(self.tx_manager.tx_status(tx_hash)),
            ProviderState::Propose => Err(L1ProviderError::ValidateTransactionConsensusBug),
            ProviderState::Pending => Err(L1ProviderError::ValidateInPendingState),
            ProviderState::Uninitialized => panic!("Uninitialized L1 provider"),
        }
    }

    // TODO: when deciding on consensus, if possible, have commit_block also tell the node if it's
    // about to [optimistically-]propose or validate the next block.
    pub fn commit_block(&mut self, _commited_txs: &[TransactionHash], _height: BlockNumber) {
        todo!(
            "Purges txs from internal buffers, if was proposer clear staging buffer, 
            reset state to Pending until we get proposing/validating notice from consensus."
        )
    }

    pub fn validation_start(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        self.validate_height(height)?;
        self.state = self.state.transition_to_validate()?;
        Ok(())
    }

    pub fn proposal_start(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        self.validate_height(height)?;
        self.state = self.state.transition_to_propose()?;
        Ok(())
    }

    /// Simple recovery from L1 and L2 reorgs by reseting the service, which rewinds L1 and L2
    /// information.
    pub async fn handle_reorg(&mut self) -> L1ProviderResult<()> {
        self.reset().await
    }

    pub async fn reset(&mut self) -> L1ProviderResult<()> {
        todo!(
            "resets internal buffers and rewinds the internal crawler _pointer_ back for ~1 \
             hour,so that the main loop will start collecting from that time gracefully. May hit \
             base layer errors when finding the latest block on l1 to 'subtract' 1 hour from. \
             Then, transition to Pending."
        );
    }

    fn validate_height(&mut self, height: BlockNumber) -> L1ProviderResult<()> {
        let next_height = self.current_height.unchecked_next();
        if height != next_height {
            return Err(L1ProviderError::UnexpectedHeight { expected: next_height, got: height });
        }
        Ok(())
    }
}

impl ComponentStarter for L1Provider {}

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

pub fn create_l1_provider(_config: L1ProviderConfig) -> L1Provider {
    L1Provider { state: ProviderState::Propose, ..Default::default() }
}
