use std::ops::RangeInclusive;

use async_trait::async_trait;
use starknet_api::block::BlockHashAndNumber;

use crate::ethereum_base_layer_contract::EthereumBaseLayerError;
use crate::{BaseLayerContract, L1BlockNumber, L1BlockReference, L1Event, PriceSample};

// FIXME: remove once we have presets. This is done because we are using dynamic dispatch for
// dependency injection, and want to avoid Box<dyn Error>, but this is breaking encapsulation.
pub type DummyBaseLayerResult<T> = Result<T, EthereumBaseLayerError>;

#[derive(Clone, Debug)]
pub struct DummyBaseLayer;

#[async_trait]
impl BaseLayerContract for DummyBaseLayer {
    type Error = EthereumBaseLayerError;
    async fn get_proved_block_at(
        &self,
        _l1_block: L1BlockNumber,
    ) -> DummyBaseLayerResult<BlockHashAndNumber> {
        Ok(Default::default())
    }

    /// Returns the latest proved block on Ethereum, where finality determines how many
    /// blocks back (0 = latest).
    async fn latest_proved_block(
        &self,
        _finality: u64,
    ) -> DummyBaseLayerResult<Option<BlockHashAndNumber>> {
        return Ok(None);
    }

    async fn events<'a>(
        &'a self,
        _block_range: RangeInclusive<u64>,
        _events: &'a [&'a str],
    ) -> DummyBaseLayerResult<Vec<L1Event>> {
        Ok(vec![])
    }

    async fn latest_l1_block_number(
        &self,
        _finality: u64,
    ) -> DummyBaseLayerResult<Option<L1BlockNumber>> {
        Ok(None)
    }

    async fn latest_l1_block(
        &self,
        _finality: u64,
    ) -> DummyBaseLayerResult<Option<L1BlockReference>> {
        return Ok(None);
    }

    async fn l1_block_at(
        &self,
        _block_number: L1BlockNumber,
    ) -> DummyBaseLayerResult<Option<L1BlockReference>> {
        Ok(None)
    }

    // Query the Ethereum base layer for the timestamp, gas price, and data gas price of a block.
    async fn get_price_sample(
        &self,
        _block_number: L1BlockNumber,
    ) -> DummyBaseLayerResult<Option<PriceSample>> {
        return Ok(None);
    }
}
