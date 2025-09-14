use std::collections::BTreeMap;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_seconds_to_duration,
};
use apollo_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CendeConfig {
    pub recorder_url: Url,
    pub skip_write_height: Option<BlockNumber>,

    // Retry policy.
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub max_retry_duration_secs: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub min_retry_interval_ms: Duration,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub max_retry_interval_ms: Duration,
}

impl Default for CendeConfig {
    fn default() -> Self {
        CendeConfig {
            recorder_url: "https://recorder_url"
                .parse()
                .expect("recorder_url must be a valid Recorder URL"),
            skip_write_height: None,
            max_retry_duration_secs: Duration::from_secs(3),
            min_retry_interval_ms: Duration::from_millis(50),
            max_retry_interval_ms: Duration::from_secs(1),
        }
    }
}

impl SerializeConfig for CendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "recorder_url",
                &self.recorder_url,
                "The URL of the Pythonic cende_recorder",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "max_retry_duration_secs",
                &self.max_retry_duration_secs.as_secs(),
                "The maximum duration (seconds) to retry the request to the recorder",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_retry_interval_ms",
                &self.min_retry_interval_ms.as_millis(),
                "The minimum waiting time (milliseconds) between retries",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_retry_interval_ms",
                &self.max_retry_interval_ms.as_millis(),
                "The maximum waiting time (milliseconds) between retries",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.skip_write_height,
            BlockNumber(0),
            "skip_write_height",
            "A height that the consensus can skip writing to Aerospike. Needed for booting up (no \
             previous height blob to write) or to handle extreme cases (all the nodes failed).",
            ParamPrivacyInput::Public,
        ));

        config
    }
}
