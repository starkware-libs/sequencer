use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHashAndNumber;
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress, Nonce};
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::L1HandlerTransaction;

pub mod ethereum_base_layer_contract;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;

#[cfg(test)]
mod base_layer_test;

/// Interface for getting data from the Starknet base contract.
#[async_trait]
pub trait BaseLayerContract {
    type Error;

    /// Get the latest Starknet block that is proved on the base layer.
    /// Optionally, require minimum confirmations.
    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> Result<Option<BlockHashAndNumber>, Self::Error>;

    /// Get specific events from the Starknet base contract between two l1 block numbers.
    async fn events(
        &self,
        from_block: u64,
        until_block: u64,
        event_identifiers: Vec<&str>,
    ) -> Result<Vec<StarknetEvent>, Self::Error>;

    async fn latest_l1_block_number(&self, finality: u64) -> Result<Option<u64>, Self::Error>;
}

/// Wraps Starknet L1 events with Starknet API types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StarknetEvent {
    LogMessageToL2 { tx: L1HandlerTransaction, fee: Fee },
    MessageToL2CancellationStarted(MessageData),
    MessageToL2Canceled(MessageData),
    ConsumedMessageToL1 { from_address: EthAddress, to_address: ContractAddress, payload: Calldata },
}

/// Shared fields in Starknet messaging.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct MessageData {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub payload: Calldata,
    pub nonce: Nonce,
}
