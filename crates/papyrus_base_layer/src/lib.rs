use std::error::Error;
use std::fmt::{Debug, Display};
use std::ops::RangeInclusive;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHashAndNumber;
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress, Nonce};
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::L1HandlerTransaction;

pub mod constants;
pub mod ethereum_base_layer_contract;

pub(crate) mod eth_events;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;

#[cfg(test)]
mod base_layer_test;

/// Interface for getting data from the Starknet base contract.
#[async_trait]
pub trait BaseLayerContract {
    type Error: Error + Display + Debug;

    /// Get the latest Starknet block that is proved on the base layer.
    /// Optionally, require minimum confirmations.
    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> Result<Option<BlockHashAndNumber>, Self::Error>;

    /// Get specific events from the Starknet base contract between two L1 block numbers.
    async fn events(
        &self,
        block_range: RangeInclusive<u64>,
        event_identifiers: &[&str],
    ) -> Result<Vec<L1Event>, Self::Error>;

    async fn latest_l1_block_number(&self, finality: u64) -> Result<Option<u64>, Self::Error>;
}

/// Wraps Starknet L1 events with Starknet API types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum L1Event {
    ConsumedMessageToL2(EventData),
    LogMessageToL2 { tx: L1HandlerTransaction, fee: Fee },
    MessageToL2CancellationStarted(EventData),
    MessageToL2Canceled(EventData),
}

/// Shared fields in Starknet messaging.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EventData {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub payload: Calldata,
    pub nonce: Nonce,
}
