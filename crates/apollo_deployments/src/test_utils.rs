use apollo_config::converters::serialize_slice_url;
use serde::{Serialize, Serializer};
use url::Url;

pub(crate) const FIX_BINARY_NAME: &str = "deployment_generator";

// TODO(Tsabary): add network secret_key instances.

#[derive(Serialize)]
pub struct SecretsConfigOverride {
    #[serde(rename = "base_layer_config.node_url")]
    base_layer_config_node_url: Url,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.base_url")]
    consensus_manager_config_eth_to_strk_oracle_config_base_url: Url,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.headers")]
    consensus_manager_config_eth_to_strk_oracle_config_headers: String,
    #[serde(
        rename = "l1_endpoint_monitor_config.ordered_l1_endpoint_urls",
        serialize_with = "serialize_slice_url_wrapper"
    )]
    l1_endpoint_monitor_config_ordered_l1_endpoint_urls: Vec<Url>,
    recorder_url: Url,
    #[serde(
        rename = "state_sync_config.central_sync_client_config.central_source_config.http_headers"
    )]
    state_sync_config_central_sync_client_config_central_source_config_http_headers: String,
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
            l1_endpoint_monitor_config_ordered_l1_endpoint_urls: vec![
                Url::parse("https://arbitrary.ordered_l1_endpoint_1.url").unwrap(),
                Url::parse("https://arbitrary.ordered_l1_endpoint_2.url").unwrap(),
            ],
            recorder_url: Url::parse("https://arbitrary.recorder.url").unwrap(),
            state_sync_config_central_sync_client_config_central_source_config_http_headers: ""
                .to_string(),
        }
    }
}

// Wrapper function for the custom `serialize_slice_url` function, to be compatible with serde's
// `serialize_with` attribute. It first applies the custom serialization logic to convert the slice
// of `Url` into a `String`, and then serializes that string.
fn serialize_slice_url_wrapper<S>(urls: &[Url], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Call the implemented serialization function
    let s = serialize_slice_url(urls); // returns String
    // Serialize the returned String
    serializer.serialize_str(&s)
}
