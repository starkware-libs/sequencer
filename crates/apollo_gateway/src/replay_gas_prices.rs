use std::sync::Mutex;
use std::time::Duration;

use apollo_mempool_types::mempool_types::TxBlockMetadata;
use reqwest::{Client, Url};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use starknet_api::block::{BlockNumber, GasPrice, GasPriceVector, NonzeroGasPrice};
use starknet_api::transaction::TransactionHash;
use tracing::warn;

#[cfg(test)]
#[path = "replay_gas_prices_test.rs"]
mod replay_gas_prices_test;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const MIN_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const MAX_RETRY_DURATION: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct RecorderBlockMetadata {
    l1_gas_price_fri: GasPrice,
    l1_data_gas_price_fri: GasPrice,
    l2_gas_price_fri: GasPrice,
}

/// Resolves the STRK gas prices of the source-chain block a replayed transaction came from.
///
/// Validating against the committed block instead cannot admit a low-headroom replayed
/// transaction at all: in a falling market its prices only drop far enough once its own block is
/// already built.
pub struct ReplayGasPricesClient {
    recorder_url: Url,
    client: ClientWithMiddleware,
    // Immutable per block, and consecutive replayed txs share one, so a single slot suffices.
    last_resolved: Mutex<Option<(BlockNumber, GasPriceVector)>>,
}

impl ReplayGasPricesClient {
    pub(crate) fn new(mut recorder_url: Url) -> Self {
        // `Url::join` replaces the last path segment unless the base ends in a slash.
        if !recorder_url.path().ends_with('/') {
            recorder_url.set_path(&format!("{}/", recorder_url.path()));
        }
        let retry_policy = ExponentialBackoff::builder()
            .retry_bounds(MIN_RETRY_INTERVAL, MAX_RETRY_INTERVAL)
            .jitter(Jitter::None)
            .build_with_total_retry_duration(MAX_RETRY_DURATION);

        Self {
            recorder_url,
            client: ClientBuilder::new(Client::new())
                .with(RetryTransientMiddleware::new_with_policy(retry_policy))
                .build(),
            last_resolved: Mutex::new(None),
        }
    }

    pub(crate) async fn strk_gas_prices(&self, tx_hash: TransactionHash) -> Option<GasPriceVector> {
        self.try_strk_gas_prices(tx_hash)
            .await
            .inspect_err(|error| {
                warn!("Falling back to the committed block's gas prices for {tx_hash}: {error}")
            })
            .ok()
    }

    async fn try_strk_gas_prices(
        &self,
        tx_hash: TransactionHash,
    ) -> Result<GasPriceVector, String> {
        let tx_block_metadata: TxBlockMetadata = self
            .try_fetch_json(&format!("echonet/get_tx_block_metadata?tx_hash={tx_hash}"))
            .await?;
        let block_number = tx_block_metadata.block_number;
        if let Some(strk_gas_prices) = self.last_resolved_for(block_number) {
            return Ok(strk_gas_prices);
        }

        let block_metadata: RecorderBlockMetadata = self
            .try_fetch_json(&format!("echonet/get_block_metadata?block_number={block_number}"))
            .await?;
        let strk_gas_prices = GasPriceVector {
            l1_gas_price: nonzero_gas_price(block_metadata.l1_gas_price_fri)?,
            l1_data_gas_price: nonzero_gas_price(block_metadata.l1_data_gas_price_fri)?,
            l2_gas_price: nonzero_gas_price(block_metadata.l2_gas_price_fri)?,
        };

        *self.last_resolved.lock().expect("Gas price memo lock was poisoned") =
            Some((block_number, strk_gas_prices.clone()));
        Ok(strk_gas_prices)
    }

    fn last_resolved_for(&self, block_number: BlockNumber) -> Option<GasPriceVector> {
        self.last_resolved
            .lock()
            .expect("Gas price memo lock was poisoned")
            .as_ref()
            .filter(|(resolved_block_number, _)| *resolved_block_number == block_number)
            .map(|(_, strk_gas_prices)| strk_gas_prices.clone())
    }

    async fn try_fetch_json<ResponseBody: DeserializeOwned>(
        &self,
        path_and_query: &str,
    ) -> Result<ResponseBody, String> {
        let url = self
            .recorder_url
            .join(path_and_query)
            .map_err(|join_error| format!("invalid recorder URL: {join_error}"))?;
        let response = self
            .client
            .get(url.as_str())
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|request_error| format!("request failed: {request_error}"))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        response
            .json::<ResponseBody>()
            .await
            .map_err(|parse_error| format!("invalid response: {parse_error}"))
    }
}

fn nonzero_gas_price(gas_price: GasPrice) -> Result<NonzeroGasPrice, String> {
    NonzeroGasPrice::new(gas_price).map_err(|error| format!("invalid gas price: {error}"))
}
