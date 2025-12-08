use std::ops::RangeInclusive;

use async_trait::async_trait;
use starknet_api::block::BlockHashAndNumber;
use tracing::info;
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

    // Check the result of a function call to the base layer. If it fails, cycle the URL and signal
    // the caller that we should try again (by returning None).
    async fn cycle_url_on_error<ReturnType>(
        &mut self,
        start_url: &Url,
        result: Result<ReturnType, B::Error>,
    ) -> Option<Result<ReturnType, B::Error>> {
        // In case we succeed, just return the (successful) result.
        if result.is_ok() {
            return Some(result);
        }
        // Get the current URL (return error in case it fails to get it).
        let current_url = match self.base_layer.get_url().await {
            Ok(url) => url,
            Err(e) => return Some(Err(e)),
        };
        // Otherwise, cycle the URL so we can try again. Return error in case it fails to cycle.
        match self.base_layer.cycle_provider_url().await {
            Ok(()) => (),
            Err(e) => return Some(Err(e)),
        };
        // Get the new URL (return error in case it fails to get it).
        let new_url = match self.base_layer.get_url().await {
            Ok(url) => url,
            Err(e) => return Some(Err(e)),
        };
        info!(
            "Cycling URL from {:?} to {:?}",
            to_safe_string(&current_url),
            to_safe_string(&new_url)
        );

        // If we've cycled back to the start URL, we need to return the last error we got.
        if &new_url == start_url {
            let error_value = result.err().expect("result is checked at start of function");
            info!(
                "Cycled back to start URL {:?}, returning error {:?}.",
                to_safe_string(start_url),
                error_value
            );
            return Some(Err(error_value));
        }
        // If we cycled but still haven't reached the start URL, we return None to signal that we
        // should try again with the new URL.
        None
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
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error. 
            }
        }
    }

    async fn latest_l1_block_number(&mut self) -> Result<L1BlockNumber, Self::Error> {
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.latest_l1_block_number().await;
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error. 
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
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error. 
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
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error. 
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
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error. 
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

fn to_safe_string(url: &Url) -> String {
    // We print only the hostnames to avoid leaking the API keys.
    url.host().map_or_else(|| "no host in url!".to_string(), |host| host.to_string())
}
