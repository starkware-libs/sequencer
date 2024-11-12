use std::collections::{BTreeMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use papyrus_config::dumping::{append_sub_config_name, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::NetworkConfig;
use serde::{Deserialize, Serialize, Serializer};

pub const DEFAULT_CONFIG_FILE_PATH: &str =
    "crates/papyrus_network/src/bin/network_stress_test/test_config.json";
pub const DEFAULT_OUTPUT_FILE_PATH: &str =
    "crates/papyrus_network/src/bin/network_stress_test/output.csv";

#[derive(Debug, Deserialize, Serialize)]
pub struct TestConfig {
    pub network_config: NetworkConfig,
    pub buffer_size: usize,
    pub message_size: usize,
    pub num_messages: u32,
    pub output_path: String,
}

impl SerializeConfig for TestConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "buffer_size",
                &self.buffer_size,
                "The buffer size for the network receiver.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "payload_size",
                &self.message_size,
                "The size of the payload for the test messages.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "message_amount",
                &self.num_messages,
                "The amount of messages to send and receive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "output_path",
                &self.output_path,
                "The path of the output file.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(append_sub_config_name(self.network_config.dump(), "network_config"));
        config
    }
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            network_config: NetworkConfig::default(),
            buffer_size: 1000,
            message_size: 1000,
            num_messages: 100000,
            output_path: DEFAULT_OUTPUT_FILE_PATH.to_string(),
        }
    }
}

impl TestConfig {
    #[allow(dead_code)]
    pub fn create_config_file() {
        let _ =
            TestConfig::default().dump_to_file(&vec![], &HashSet::new(), DEFAULT_CONFIG_FILE_PATH);
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub id: u32,
    #[serde(serialize_with = "system_time_to_millis")]
    pub start_time: SystemTime,
    #[serde(serialize_with = "system_time_to_millis")]
    pub end_time: SystemTime,
    pub duration: u128,
}

pub fn system_time_to_millis<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let duration_since_epoch =
        time.duration_since(UNIX_EPOCH).map_err(serde::ser::Error::custom)?;
    let millis =
        duration_since_epoch.as_secs() * 1000 + u64::from(duration_since_epoch.subsec_millis());
    serializer.serialize_u64(millis)
}
