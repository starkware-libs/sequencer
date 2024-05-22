use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::sync::Arc;

use axum::body::{Body, Bytes, HttpBody};
use axum::http::{Request, StatusCode};
use blockifier::blockifier::block::BlockInfo;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use pretty_assertions::assert_str_eq;
use rstest::rstest;
use starknet_gateway::config::{
    GatewayNetworkConfig, StatefulTransactionValidatorConfig, StatelessTransactionValidatorConfig,
};
use starknet_gateway::gateway::Gateway;
use starknet_gateway::state_reader_test_utils::{TestStateReader, TestStateReaderFactory};
use starknet_mempool_types::mempool_types::{
    GatewayNetworkComponent, GatewayToMempoolMessage, MempoolToGatewayMessage,
};
use tokio::sync::mpsc::channel;
use tower::ServiceExt;

const TEST_FILES_FOLDER: &str = "./tests/fixtures";

// TODO(Ayelet): Replace the use of the JSON files with generated instances, then serialize these
// into JSON for testing.
#[rstest]
#[case::declare(&Path::new(TEST_FILES_FOLDER).join("declare_v3.json"), "DECLARE")]
#[case::deploy_account(
    &Path::new(TEST_FILES_FOLDER).join("deploy_account_v3.json"),
    "DEPLOY_ACCOUNT"
)]
#[case::invoke(&Path::new(TEST_FILES_FOLDER).join("invoke_v3.json"), "INVOKE")]
#[tokio::test]
async fn test_routes(#[case] json_file_path: &Path, #[case] expected_response: &str) {
    let tx_json = fs::read_to_string(json_file_path).unwrap();
    let request = Request::post("/add_tx")
        .header("content-type", "application/json")
        .body(Body::from(tx_json))
        .unwrap();

    let response = check_request(request, StatusCode::OK).await;

    assert_str_eq!(expected_response, String::from_utf8_lossy(&response));
}

#[rstest]
#[tokio::test]
#[should_panic]
// FIXME: Currently is_alive is not implemented, fix this once it is implemented.
async fn test_is_alive() {
    let request = Request::get("/is_alive").body(Body::empty()).unwrap();
    // Status code doesn't matter, this panics ATM.
    check_request(request, StatusCode::default()).await;
}

async fn check_request(request: Request<Body>, status_code: StatusCode) -> Bytes {
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let port = 3000;
    let network_config: GatewayNetworkConfig = GatewayNetworkConfig { ip, port };

    let stateless_transaction_validator_config = StatelessTransactionValidatorConfig {
        max_calldata_length: 1000,
        max_signature_length: 2,
        ..Default::default()
    };

    // The  `_rx_gateway_to_mempool`   is retained to keep the channel open, as dropping it would
    // prevent the sender from transmitting messages.
    let (tx_gateway_to_mempool, _rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (_, rx_mempool_to_gateway) = channel::<MempoolToGatewayMessage>(1);
    let network_component =
        GatewayNetworkComponent::new(tx_gateway_to_mempool, rx_mempool_to_gateway);
    let stateful_transaction_validator_config =
        StatefulTransactionValidatorConfig::create_for_testing();
    let state_reader_factory = Arc::new(TestStateReaderFactory {
        state_reader: TestStateReader {
            block_info: BlockInfo::create_for_testing(),
            // TODO(yael 16/5/2024): create a test state that will make the tx pass validations
            blockifier_state_reader: DictStateReader::default(),
        },
    });

    // TODO: Add fixture.
    let gateway = Gateway {
        network_config,
        stateless_transaction_validator_config,
        stateful_transaction_validator_config,
        network_component,
        state_reader_factory,
    };

    let response = gateway.app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), status_code);

    response.into_body().collect().await.unwrap().to_bytes()
}
