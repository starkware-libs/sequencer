use std::error::Error;
use std::fmt::{Debug, Display};
use std::ops::RangeInclusive;

use alloy::primitives::FixedBytes;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
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

pub type L1BlockNumber = u64;
pub type L1BlockHash = [u8; 32];

#[cfg(any(feature = "testing", test))]
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MockError {}

/// Interface for getting data from the Starknet base contract.
#[cfg_attr(any(feature = "testing", test), automock(type Error = MockError;))]
#[async_trait]
pub trait BaseLayerContract {
    type Error: Error + PartialEq + Display + Debug;

    /// Get the latest Starknet block that is proved on the base layer at a specific L1 block
    /// number. If the number is too low, return an error.
    async fn get_proved_block_at(
        &self,
        l1_block: L1BlockNumber,
    ) -> Result<BlockHashAndNumber, Self::Error>;

    /// Get the latest Starknet block that is proved on the base layer with minimum number of
    /// confirmations (for no confirmations, pass `0`).
    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> Result<Option<BlockHashAndNumber>, Self::Error>;

    async fn latest_l1_block_number(
        &self,
        finality: u64,
    ) -> Result<Option<L1BlockNumber>, Self::Error>;

    async fn latest_l1_block(&self, finality: u64)
    -> Result<Option<L1BlockReference>, Self::Error>;

    async fn l1_block_at(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockReference>, Self::Error>;

    /// Get specific events from the Starknet base contract between two L1 block numbers.
    async fn events<'a>(
        &'a self,
        block_range: RangeInclusive<L1BlockNumber>,
        event_identifiers: &'a [&'a str],
    ) -> Result<Vec<L1Event>, Self::Error>;

    async fn get_block_header(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error>;
}

/// Reference to an L1 block, extend as needed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct L1BlockReference {
    pub number: L1BlockNumber,
    pub hash: L1BlockHash,
}

/// A struct with some of the fields of the L1 block header. Extend as needed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct L1BlockHeader {
    pub number: L1BlockNumber,
    pub hash: L1BlockHash,
    pub parent_hash: L1BlockHash,
    pub timestamp: u64,
    pub base_fee_per_gas: u128,
    pub blob_fee: u128,
}

/// Wraps Starknet L1 events with Starknet API types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum L1Event {
    ConsumedMessageToL2(EventData),
    // TODO(Arni): Consider adding the l1_tx_hash to all variants of L1 Event.
    LogMessageToL2 { tx: L1HandlerTransaction, fee: Fee, l1_tx_hash: Option<FixedBytes<32>> },
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

impl Display for EventData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EventData {{ from_address: {:?}, to_address: {:?}, calldata: <omitted>, \
             entry_point_selector: {}, nonce: {} }}",
            self.from_address, self.to_address, self.entry_point_selector, self.nonce
        )
    }
}
