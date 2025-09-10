use std::collections::BTreeMap;

use apollo_config::converters::{
    deserialize_optional_list_with_url_and_headers,
    serialize_optional_list_with_url_and_headers,
    UrlAndHeaders,
};
use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Validate)]
pub struct EthToStrkOracleConfig {
    #[serde(deserialize_with = "deserialize_optional_list_with_url_and_headers")]
    pub url_header_list: Option<Vec<UrlAndHeaders>>,
    pub lag_interval_seconds: u64,
    pub max_cache_size: usize,
    pub query_timeout_sec: u64,
}

impl SerializeConfig for EthToStrkOracleConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "url_header_list",
                &serialize_optional_list_with_url_and_headers(&self.url_header_list),
                "A list of Url+HTTP headers for the eth to strk oracle. \
                 The url is followed by a comma and then headers as key^value pairs, separated by commas. \
                 For example: `https://api.example.com/api,key1^value1,key2^value2`. \
                 Each URL+headers is separated by a pipe `|` character. \
                 The `timestamp` parameter is appended dynamically when making requests, in order \
                 to have a stable mapping from block timestamp to conversion rate. ",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "lag_interval_seconds",
                &self.lag_interval_seconds,
                "The size of the interval (seconds) that the eth to strk rate is taken on. The \
                 lag refers to the fact that the interval `[T, T+k)` contains the conversion rate \
                 for queries in the interval `[T+k, T+2k)`. Should be configured in alignment \
                 with relevant query parameters in `url_header_list`, if required.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_cache_size",
                &self.max_cache_size,
                "The maximum number of cached conversion rates.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "query_timeout_sec",
                &self.query_timeout_sec,
                "The timeout (seconds) for the query to the eth to strk oracle.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for EthToStrkOracleConfig {
    fn default() -> Self {
        Self {
            url_header_list: Some(vec![UrlAndHeaders {
                url: Url::parse("https://api.example.com/api").expect("Invalid URL"),
                headers: BTreeMap::new(),
            }]),
            lag_interval_seconds: 1,
            max_cache_size: 100,
            query_timeout_sec: 3,
        }
    }
}

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
    #[validate]
    pub eth_to_strk_oracle_config: EthToStrkOracleConfig,
}

impl Default for L1GasPriceProviderConfig {
    fn default() -> Self {
        const MEAN_NUMBER_OF_BLOCKS: u64 = 300;
        Self {
            number_of_blocks_for_mean: MEAN_NUMBER_OF_BLOCKS,
            lag_margin_seconds: 60,
            storage_limit: usize::try_from(10 * MEAN_NUMBER_OF_BLOCKS).unwrap(),
            max_time_gap_seconds: 900, // 15 minutes
            eth_to_strk_oracle_config: EthToStrkOracleConfig::default(),
        }
    }
}

impl SerializeConfig for L1GasPriceProviderConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from([
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
        ]);
        config.extend(prepend_sub_config_name(
            self.eth_to_strk_oracle_config.dump(),
            "eth_to_strk_oracle_config",
        ));
        config
    }
}
