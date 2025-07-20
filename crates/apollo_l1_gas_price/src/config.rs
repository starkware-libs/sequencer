use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct L1GasPriceProviderConfig {
    // TODO(guyn): these two fields need to go into VersionedConstants.
    pub number_of_blocks_for_mean: u64,
    // Use seconds not Duration since seconds is the basic quanta of time for both Starknet and
    // Ethereum.
    pub lag_margin_seconds: u64,
    pub storage_limit: usize,
    // Maximum valid time gap between the requested timestamp and the last price sample in seconds.
    pub max_time_gap_seconds: u64,
}

impl Default for L1GasPriceProviderConfig {
    fn default() -> Self {
        const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
        Self {
            number_of_blocks_for_mean: MEAN_NUMBER_OF_BLOCKS,
            lag_margin_seconds: 60,
            storage_limit: usize::try_from(10 * MEAN_NUMBER_OF_BLOCKS).unwrap(),
            max_time_gap_seconds: 900, // 15 minutes
        }
    }
}

impl SerializeConfig for L1GasPriceProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "number_of_blocks_for_mean",
                &self.number_of_blocks_for_mean,
                "Number of blocks to use for the mean gas price calculation",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "lag_margin_seconds",
                &self.lag_margin_seconds,
                "Difference between the time of the block from L1 used to calculate the gas price \
                 and the time of the L2 block this price is used in",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "storage_limit",
                &self.storage_limit,
                "Maximum number of L1 blocks to keep cached",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_time_gap_seconds",
                &self.max_time_gap_seconds,
                "Maximum valid time gap between the requested timestamp and the last price sample \
                 in seconds",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
