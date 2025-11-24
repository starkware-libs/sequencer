use std::ops::RangeInclusive;

use async_trait::async_trait;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::{BaseLayerContract, L1BlockHeader, L1BlockNumber, L1BlockReference, L1Event};

#[cfg(test)]
#[path = "cyclic_base_layer_wrapper_test.rs"]
pub mod cyclic_base_layer_wrapper_test;

pub struct CyclicBaseLayerWrapper<B: BaseLayerContract + Send + Sync> {
    base_layer: B,
}

impl<B: BaseLayerContract + Send + Sync> CyclicBaseLayerWrapper<B> {
    pub fn new(base_layer: B) -> Self {
        Self { base_layer }
    }
}

#[async_trait]
impl<B: BaseLayerContract + Send + Sync> BaseLayerContract for CyclicBaseLayerWrapper<B> {
    type Error = B::Error;

    async fn get_proved_block_at(
        &mut self,
        l1_block: L1BlockNumber,
    ) -> Result<BlockHashAndNumber, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.get_proved_block_at(l1_block).await;
            if result.is_ok() {
                return result;
            }
            self.base_layer.cycle_provider_url().await?;
            if self.base_layer.get_url().await? == start_url {
                return result;
            }
        }
    }

    async fn latest_l1_block_number(&mut self) -> Result<L1BlockNumber, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.latest_l1_block_number().await;
            if result.is_ok() {
                return result;
            }
            self.base_layer.cycle_provider_url().await?;
            if self.base_layer.get_url().await? == start_url {
                return result;
            }
        }
    }

    async fn l1_block_at(
        &mut self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockReference>, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.l1_block_at(block_number).await;
            if result.is_ok() {
                return result;
            }
            self.base_layer.cycle_provider_url().await?;
            if self.base_layer.get_url().await? == start_url {
                return result;
            }
        }
    }

    async fn events<'a>(
        &'a mut self,
        block_range: RangeInclusive<L1BlockNumber>,
        event_identifiers: &'a [&'a str],
    ) -> Result<Vec<L1Event>, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.events(block_range.clone(), event_identifiers).await;
            if result.is_ok() {
                return result;
            }
            self.base_layer.cycle_provider_url().await?;
            if self.base_layer.get_url().await? == start_url {
                return result;
            }
        }
    }

    async fn get_block_header(
        &mut self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.get_block_header(block_number).await;
            if result.is_ok() {
                return result;
            }
            self.base_layer.cycle_provider_url().await?;
            if self.base_layer.get_url().await? == start_url {
                return result;
            }
        }
    }

    async fn get_block_header_immutable(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error> {
        self.base_layer.get_block_header_immutable(block_number).await
    }

    async fn get_url(&self) -> Result<Url, Self::Error> {
        self.base_layer.get_url().await
    }

    async fn set_provider_url(&mut self, url: Url) -> Result<(), Self::Error> {
        self.base_layer.set_provider_url(url).await
    }

    async fn cycle_provider_url(&mut self) -> Result<(), Self::Error> {
        self.base_layer.cycle_provider_url().await
    }
}
