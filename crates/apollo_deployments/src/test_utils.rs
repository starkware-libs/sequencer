use apollo_config::converters::{serialize_optional_vec_u8, serialize_slice};
use serde::{Serialize, Serializer};
use url::Url;

pub(crate) const FIX_BINARY_NAME: &str = "deployment_generator";

#[derive(Serialize)]
pub struct SecretsConfigOverride {
    #[serde(rename = "base_layer_config.node_url")]
    base_layer_config_node_url: Url,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.base_url")]
    consensus_manager_config_eth_to_strk_oracle_config_base_url: Url,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.headers")]
    consensus_manager_config_eth_to_strk_oracle_config_headers: String,
    #[serde(
        rename = "consensus_manager_config.network_config.secret_key",
        serialize_with = "serialize_optional_vec_u8_wrapper"
    )]
    consensus_manager_config_network_config_secret_key: Option<Vec<u8>>,
    #[serde(
        rename = "l1_endpoint_monitor_config.ordered_l1_endpoint_urls",
        serialize_with = "serialize_slice_wrapper"
    )]
    l1_endpoint_monitor_config_ordered_l1_endpoint_urls: Vec<Url>,
    #[serde(
        rename = "mempool_p2p_config.network_config.secret_key",
        serialize_with = "serialize_optional_vec_u8_wrapper"
    )]
    mempool_p2p_config_network_config_secret_key: Option<Vec<u8>>,
    recorder_url: Url,
    #[serde(
        rename = "state_sync_config.central_sync_client_config.central_source_config.http_headers"
    )]
    state_sync_config_central_sync_client_config_central_source_config_http_headers: String,
    #[serde(
        rename = "state_sync_config.network_config.secret_key",
        serialize_with = "serialize_optional_vec_u8_wrapper"
    )]
    state_sync_config_network_config_secret_key: Option<Vec<u8>>,
}

impl Default for SecretsConfigOverride {
    fn default() -> Self {
        Self {
            base_layer_config_node_url: Url::parse("https://arbitrary.url.com").unwrap(),
            consensus_manager_config_eth_to_strk_oracle_config_base_url: Url::parse(
                "https://arbitrary.eth_to_strk_oracle.url",
            )
            .unwrap(),
            consensus_manager_config_eth_to_strk_oracle_config_headers: "".to_string(),
            consensus_manager_config_network_config_secret_key: None,
            l1_endpoint_monitor_config_ordered_l1_endpoint_urls: vec![
                Url::parse("https://arbitrary.ordered_l1_endpoint_1.url").unwrap(),
                Url::parse("https://arbitrary.ordered_l1_endpoint_2.url").unwrap(),
            ],
            mempool_p2p_config_network_config_secret_key: None,
            recorder_url: Url::parse("https://arbitrary.recorder.url").unwrap(),
            state_sync_config_central_sync_client_config_central_source_config_http_headers: ""
                .to_string(),
            state_sync_config_network_config_secret_key: None,
        }
    }
}

// Wrapper function for the custom `serialize_slice` function, to be compatible with serde's
// `serialize_with` attribute. It first applies the custom serialization logic to convert the slice
// of `Url` into a `String`, and then serializes that string.
fn serialize_slice_wrapper<S>(urls: &[Url], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Call the implemented custom serialization function
    let s = serialize_slice(urls);
    // Serialize the returned String
    serializer.serialize_str(&s)
}

// Wrapper function for the custom `serialize_optional_vec_u8` function, to be compatible with
// serde's `serialize_with` attribute. It first applies the custom serialization logic to convert
// the optional u8 vector into a `String`, and then serializes that string.
pub fn serialize_optional_vec_u8_wrapper<S>(
    value: &Option<Vec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Call the implemented custom serialization function
    let s = serialize_optional_vec_u8(value);
    // Serialize the returned String
    serializer.serialize_str(&s)
}
