use std::ops::RangeInclusive;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apollo_config::secrets::Sensitive;
use apollo_metrics::metrics::LossyIntoF64;
use async_trait::async_trait;
use starknet_api::block::BlockHashAndNumber;
use tracing::info;
use url::Url;

use crate::metrics::{
    ScraperLabel,
    L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS,
    LABEL_NAME_SCRAPER,
};
use crate::{BaseLayerContract, L1BlockHeader, L1BlockNumber, L1BlockReference, L1Event};

#[cfg(test)]
#[path = "cyclic_base_layer_wrapper_test.rs"]
pub mod cyclic_base_layer_wrapper_test;

#[derive(Debug)]
pub struct CyclicBaseLayerWrapper<B: BaseLayerContract + Send + Sync> {
    base_layer: B,
    retry_primary_interval: Duration,
    last_primary_retry: tokio::time::Instant,
    scraper: ScraperLabel,
    // Unix timestamp (seconds) at which the primary endpoint first became non-functional in the
    // current outage; None when the primary is healthy.
    primary_down_since: Option<u64>,
}

impl<B: BaseLayerContract + Send + Sync> CyclicBaseLayerWrapper<B> {
    pub fn new(base_layer: B, retry_primary_interval: Duration, scraper: ScraperLabel) -> Self {
        crate::metrics::register_metrics();
        Self {
            base_layer,
            retry_primary_interval,
            last_primary_retry: tokio::time::Instant::now(),
            scraper,
            primary_down_since: None,
        }
    }

    // Retries the primary endpoint once the interval has elapsed since we left it. Does nothing
    // while already on the primary, so the timer is untouched until a failover moves us off it.
    async fn retry_primary_if_due(&mut self) -> Result<(), B::Error> {
        if self.base_layer.is_at_primary().await? {
            return Ok(());
        }
        if self.last_primary_retry.elapsed() >= self.retry_primary_interval {
            // Advance the clock before attempting the reset, so that whether or not the primary is
            // back up we wait a full interval before retrying it again. This avoids churning the
            // primary on every access while it is still down.
            self.last_primary_retry = tokio::time::Instant::now();
            self.base_layer.reset_provider_url_to_primary().await?;
        }
        Ok(())
    }

    // Check the result of a function call to the base layer. If it fails, cycle the URL and signal
    // the caller that we should try again (by returning None).
    async fn cycle_url_on_error<ReturnType: std::fmt::Debug>(
        &mut self,
        start_url: &Sensitive<Url>,
        result: Result<ReturnType, B::Error>,
    ) -> Option<Result<ReturnType, B::Error>> {
        // In case we succeed, just return the (successful) result.
        if result.is_ok() {
            // If we succeeded while on the primary, the primary is healthy — clear the down-since
            // state and reset the gauge to 0. An optimistic reset to a still-down primary (via
            // retry_primary_if_due) is NOT treated as healthy here; only an actual successful call
            // while is_at_primary() confirms health.
            if self.base_layer.is_at_primary().await.unwrap_or(false) {
                self.primary_down_since = None;
                L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS
                    .set(0f64, &[(LABEL_NAME_SCRAPER, self.scraper.into())]);
            }
            return Some(result);
        }
        // Get the current URL (return error in case it fails to get it).
        let current_url_result = self.base_layer.get_url().await;
        let Ok(current_url) = current_url_result else {
            return Some(Err(current_url_result.expect_err("result is checked at let-else")));
        };
        // Record whether we are about to leave the primary, before cycling away from it.
        let is_at_primary_result = self.base_layer.is_at_primary().await;
        let Ok(was_at_primary) = is_at_primary_result else {
            return Some(Err(is_at_primary_result.expect_err("result is checked at let-else")));
        };
        // Otherwise, cycle the URL so we can try again. Return error in case it fails to cycle.
        let cycle_url_result = self.base_layer.cycle_provider_url().await;
        let Ok(()) = cycle_url_result else {
            return Some(Err(cycle_url_result.expect_err("result is checked at let-else")));
        };
        // Restart the retry-primary clock only when this failover leaves the primary, so the wait
        // is measured from when we left it; cycling between backups must not push the retry out.
        if was_at_primary {
            self.last_primary_retry = tokio::time::Instant::now();
            // Record the unix timestamp at which the primary first went down. Do not overwrite an
            // existing value: the gauge must reflect when the outage started, not the most recent
            // cycle that happened to pass through the primary.
            if self.primary_down_since.is_none() {
                let down_since =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                self.primary_down_since = Some(down_since);
                L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS
                    .set(down_since.into_f64(), &[(LABEL_NAME_SCRAPER, self.scraper.into())]);
            }
        }
        // Get the new URL (return error in case it fails to get it).
        let new_url_result = self.base_layer.get_url().await;
        let Ok(new_url) = new_url_result else {
            return Some(Err(new_url_result.expect_err("result is checked at let-else")));
        };
        info!("Cycling URL from {:?} to {:?}", &current_url, &new_url);

        // If we've cycled back to the start URL, we need to return the last error we got.
        if &new_url == start_url {
            info!("Cycled back to start URL {:?}, returning error {:?}.", start_url, result);
            return Some(result);
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
        self.retry_primary_if_due().await?;
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.get_proved_block_at(l1_block).await;
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error.
            }
        }
    }

    async fn latest_l1_block_number(&mut self) -> Result<L1BlockNumber, Self::Error> {
        self.retry_primary_if_due().await?;
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
        self.retry_primary_if_due().await?;
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
        self.retry_primary_if_due().await?;
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
        self.retry_primary_if_due().await?;
        let start_url = self.base_layer.get_url().await?;
        loop {
            let result = self.base_layer.get_block_header(block_number).await;
            if let Some(result) = self.cycle_url_on_error(&start_url, result).await {
                return result; // Could return a success or an error.
            }
        }
    }

    // Takes &self so it cannot cycle or retry endpoints; callers needing resilience use the &mut
    // self methods.
    async fn get_block_header_immutable(
        &self,
        block_number: L1BlockNumber,
    ) -> Result<Option<L1BlockHeader>, Self::Error> {
        self.base_layer.get_block_header_immutable(block_number).await
    }

    async fn get_url(&self) -> Result<Sensitive<Url>, Self::Error> {
        self.base_layer.get_url().await
    }

    async fn set_provider_url(&mut self, url: Sensitive<Url>) -> Result<(), Self::Error> {
        self.base_layer.set_provider_url(url).await
    }

    async fn cycle_provider_url(&mut self) -> Result<(), Self::Error> {
        self.base_layer.cycle_provider_url().await
    }

    async fn reset_provider_url_to_primary(&mut self) -> Result<(), Self::Error> {
        self.base_layer.reset_provider_url_to_primary().await
    }

    async fn is_at_primary(&self) -> Result<bool, Self::Error> {
        self.base_layer.is_at_primary().await
    }
}
