use std::collections::BTreeMap;

use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use thiserror::Error;
use validator::Validate;

// TODO(guyn): both these constants need to go into VersionedConstants.
pub const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
pub const LAG_MARGIN_SECONDS: u32 = 60;

// TODO(guyn, Gilda): consider moving this to starknet_l1_provider_types/lib.rs?
// This is an interface that allows sharing the provider with the scraper across threads.
pub trait L1GasPriceProviderClient: Send + Sync {
    fn add_price_info(
        &self,
        height: BlockNumber,
        timestamp: BlockTimestamp,
        gas_price: u128,
        data_gas_price: u128,
    ) -> Result<(), L1GasPriceProviderError>;

    fn get_price_info(&self, timestamp: BlockTimestamp) -> Result<u64, L1GasPriceProviderError>;
}

#[derive(Clone, Debug, Error)]
pub enum L1GasPriceProviderError {
    #[error("Failed to add price info: {0}")]
    AddPriceInfoError(String),
    #[error("Failed to get price info: {0}")]
    GetPriceInfoError(String),
}

// TODO(guyn): add the concrete implementation of the gas price provider client here.
