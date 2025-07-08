use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_network::NetworkConfig;
use libp2p::identity::Keypair;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize, Serializer};

pub const BOOTSTRAP_CONFIG_FILE_PATH: &str =
    "crates/apollo_network/src/bin/network_stress_test/bootstrap_test_config.json";
pub const BOOTSTRAP_OUTPUT_FILE_PATH: &str =
    "crates/apollo_network/src/bin/network_stress_test/bootstrap_output.csv";
pub const DEFAULT_CONFIG_FILE_PATH: &str =
    "crates/apollo_network/src/bin/network_stress_test/test_config.json";
pub const DEFAULT_OUTPUT_FILE_PATH: &str =
    "crates/apollo_network/src/bin/network_stress_test/output.csv";

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
                "message_size",
                &self.message_size,
                "The size of the payload for the test messages.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_messages",
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
        config.extend(prepend_sub_config_name(self.network_config.dump(), "network_config"));
        config
    }
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            network_config: NetworkConfig::default(),
            buffer_size: 1000,
            message_size: 1000,
            num_messages: 10000,
            output_path: BOOTSTRAP_OUTPUT_FILE_PATH.to_string(),
        }
    }
}

impl TestConfig {
    #[allow(dead_code)]
    pub fn create_config_files() {
        let secret_key = vec![0; 32];
        let keypair = Keypair::ed25519_from_bytes(secret_key.clone()).unwrap();
        let peer_id = keypair.public().to_peer_id();

        let _ = TestConfig {
            network_config: NetworkConfig {
                port: 10000,
                secret_key: Some(secret_key),
                ..Default::default()
            },
            ..Default::default()
        }
        .dump_to_file(&vec![], &HashSet::new(), BOOTSTRAP_CONFIG_FILE_PATH);
        let _ = TestConfig {
            network_config: NetworkConfig {
                port: 10002,
                bootstrap_peer_multiaddr: Some(vec![
                    Multiaddr::from_str(&format!("/ip4/127.0.0.1/udp/10000/quic-v1/p2p/{peer_id}"))
                        .unwrap(),
                ]),
                ..Default::default()
            },
            output_path: DEFAULT_OUTPUT_FILE_PATH.to_string(),
            ..Default::default()
        }
        .dump_to_file(&vec![], &HashSet::new(), DEFAULT_CONFIG_FILE_PATH);
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Record {
    pub peer_id: String,
    pub id: u32,
    #[serde(serialize_with = "serialize_system_time_as_u128_millis")]
    pub start_time: SystemTime,
    #[serde(serialize_with = "serialize_system_time_as_u128_millis")]
    pub end_time: SystemTime,
    pub duration: i128,
}

pub fn serialize_system_time_as_u128_millis<S>(
    time: &SystemTime,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let duration_since_epoch =
        time.duration_since(UNIX_EPOCH).map_err(serde::ser::Error::custom)?;
    let millis = duration_since_epoch.as_millis();
    serializer.serialize_u128(millis)
}
