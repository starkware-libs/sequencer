use std::collections::{BTreeMap, VecDeque};

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use thiserror::Error;
use validator::Validate;

// TODO(guyn): both these constants need to go into VersionedConstants.
pub const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
pub const LAG_MARGIN_SECONDS: u64 = 60;

#[cfg(test)]
#[path = "l1_gas_price_provider_tests.rs"]
pub mod l1_gas_price_provider_tests;

// TODO(guyn, Gilad): consider moving this to starknet_l1_provider_types/lib.rs?
// This is an interface that allows sharing the provider with the scraper across threads.
pub trait L1GasPriceProviderClient: Send + Sync {
    fn add_price_info(
        &self,
        height: BlockNumber,
        timestamp: BlockTimestamp,
        gas_price: u128,
        data_gas_price: u128,
    ) -> Result<(), L1GasPriceProviderError>;

    fn get_price_info(
        &self,
        timestamp: BlockTimestamp,
    ) -> Result<(u128, u128), L1GasPriceProviderError>;
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    AddPriceInfoError(String),
    #[error("Failed to get price info: {0}")]
    GetPriceInfoError(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceProviderConfig {
    pub storage_limit: usize,
}

impl Default for L1GasPriceProviderConfig {
    fn default() -> Self {
        Self { storage_limit: usize::try_from(10 * MEAN_NUMBER_OF_BLOCKS).unwrap() }
    }
}

impl SerializeConfig for L1GasPriceProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "storage_limit",
            &self.storage_limit,
            "The maximum number of blocks to store in the gas price provider's buffer.",
            ParamPrivacyInput::Public,
        )])
    }
}

#[derive(Clone, Debug, Default)]
pub struct L1GasPriceProvider {
    config: L1GasPriceProviderConfig,
    data: VecDeque<(BlockNumber, BlockTimestamp, u128, u128)>,
}

// TODO(guyn): remove the dead code attribute when we use this.
#[allow(dead_code)]
impl L1GasPriceProvider {
    pub fn new(config: L1GasPriceProviderConfig) -> Self {
        Self { config, data: VecDeque::new() }
    }

    pub fn add_price_info(
        &mut self,
        height: BlockNumber,
        timestamp: BlockTimestamp,
        gas_price: u128,
        data_gas_price: u128,
    ) -> Result<(), L1GasPriceProviderError> {
        self.data.push_back((height, timestamp, gas_price, data_gas_price));
        if self.data.len() >= self.config.storage_limit {
            self.data.pop_front();
        }
        Ok(())
    }

    pub fn get_price_info(
        &mut self,
        timestamp: BlockTimestamp,
    ) -> Result<(u128, u128), L1GasPriceProviderError> {
        let mut gas_price = 0;
        let mut data_gas_price = 0;

        let index_after_timestamp =
            self.data.iter().position(|(_, ts, _, _)| ts.0 >= timestamp.0 - LAG_MARGIN_SECONDS);
        if let Some(index) = index_after_timestamp {
            let number = usize::try_from(MEAN_NUMBER_OF_BLOCKS).unwrap();
            if index < number {
                return Err(L1GasPriceProviderError::GetPriceInfoError(format!(
                    "Insufficient number of block prices were cached ({})",
                    index
                )));
            }
            // Go over all elements between index-number to index.
            for (_height, _ts, gp, dgp) in self.data.iter().skip(index - number + 1).take(number) {
                gas_price += gp;
                data_gas_price += dgp;
            }
            Ok((
                gas_price / u128::from(MEAN_NUMBER_OF_BLOCKS),
                data_gas_price / u128::from(MEAN_NUMBER_OF_BLOCKS),
            ))
        } else {
            Err(L1GasPriceProviderError::GetPriceInfoError(format!(
                "No block price data from time {} - {}s",
                timestamp.0, LAG_MARGIN_SECONDS
            )))
        }
    }
}
