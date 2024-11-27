use async_trait::async_trait;
use starknet_api::block::{BlockHash, BlockNumber};

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
    ) -> Result<Option<(BlockNumber, BlockHash)>, Self::Error>;
}
