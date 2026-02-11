use crate::config::ProverConfig;
use crate::server::config::ServiceConfig;
use crate::server::rpc_impl::ProvingRpcServerImpl;
use crate::server::rpc_trait::ProvingRpcServer;

fn service_config_with_valid_url() -> ServiceConfig {
    ServiceConfig {
        prover_config: ProverConfig {
            rpc_node_url: "http://localhost:8545".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn spec_version_matches_starknet_rpc_v010() {
    let server = ProvingRpcServerImpl::from_config(&service_config_with_valid_url());
    let version = server.spec_version().await.expect("specVersion should succeed");
    assert_eq!(version, "0.10.0");
}

#[test]
#[should_panic(expected = "Invalid RPC node URL in config")]
fn from_config_panics_on_invalid_rpc_url() {
    let mut config = service_config_with_valid_url();
    config.prover_config.rpc_node_url = "not a url".to_string();

    let _ = ProvingRpcServerImpl::from_config(&config);
}
