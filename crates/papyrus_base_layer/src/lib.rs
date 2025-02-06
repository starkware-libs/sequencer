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

pub type L1BlockNumber = u64;

/// Interface for getting data from the Starknet base contract.
#[async_trait]
pub trait BaseLayerContract {
    type Error: Error + Display + Debug;

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
    async fn events(
        &self,
        block_range: RangeInclusive<L1BlockNumber>,
        event_identifiers: &[&str],
    ) -> Result<Vec<L1Event>, Self::Error>;

    async fn get_price_sample(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<PriceSample>, Self::Error>;
}

/// A struct that holds together the data on the base layer's gas prices, for a given timestamp.
pub struct PriceSample {
    pub timestamp: u64,
    // Fee is stored as u128 for compatibility with any base layer.
    pub base_fee_per_gas: u128,
    pub blob_fee: u128,
}

/// Reference to an L1 block, extend as needed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct L1BlockReference {
    pub number: L1BlockNumber,
    pub hash: [u8; 32],
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
