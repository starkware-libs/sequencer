use serde::Serialize;
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
    #[serde(rename = "l1_endpoint_monitor_config.ordered_l1_endpoint_urls")]
    l1_endpoint_monitor_config_ordered_l1_endpoint_urls: String,
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
            l1_endpoint_monitor_config_ordered_l1_endpoint_urls:
                "https://arbitrary.ordered_l1_endpoint.url".to_string(),
            recorder_url: Url::parse("https://arbitrary.recorder.url").unwrap(),
            state_sync_config_central_sync_client_config_central_source_config_http_headers: ""
                .to_string(),
        }
    }
}
