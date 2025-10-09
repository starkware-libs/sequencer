use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_vec,
    serialize_slice,
};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq, Eq)]
pub struct L1EndpointMonitorConfig {
    #[serde(deserialize_with = "deserialize_vec")]
    pub ordered_l1_endpoint_urls: Vec<Url>,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub timeout_millis: Duration,
}

impl Default for L1EndpointMonitorConfig {
    fn default() -> Self {
        Self {
            ordered_l1_endpoint_urls: vec![
                Url::parse("https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY").unwrap(),
                Url::parse("https://eth-mainnet.g.alchemy.com/v2/YOUR_ALCHEMY_API_KEY").unwrap(),
            ],
            timeout_millis: Duration::from_millis(1000),
        }
    }
}

impl SerializeConfig for L1EndpointMonitorConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "ordered_l1_endpoint_urls",
                &serialize_slice(&self.ordered_l1_endpoint_urls),
                "Ordered list of L1 endpoint URLs, used in order, cyclically, switching if the \
                 current one is non-operational.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "timeout_millis",
                &self.timeout_millis.as_millis(),
                "The timeout (milliseconds) for a query of the L1 base layer",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
