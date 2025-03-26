use std::net::{IpAddr, Ipv4Addr};
use std::panic::AssertUnwindSafe;

use apollo_gateway_types::communication::{GatewayClientError, MockGatewayClient};
use apollo_gateway_types::errors::{GatewayError, GatewaySpecError};
use apollo_gateway_types::gateway_types::{
    DeclareGatewayOutput,
    DeployAccountGatewayOutput,
    GatewayOutput,
    InvokeGatewayOutput,
};
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_sequencer_infra::component_client::ClientError;
use axum::body::{Bytes, HttpBody};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use blockifier_test_utils::cairo_versions::CairoVersion;
use futures::FutureExt;
use jsonrpsee::types::error::ErrorCode;
use jsonrpsee::types::ErrorObjectOwned;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use rstest::rstest;
use serde_json::Value;
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address, tx_hash};
use starknet_types_core::felt::Felt;
use tracing_test::traced_test;

use crate::config::HttpServerConfig;
use crate::errors::HttpServerError;
use crate::http_server::CLIENT_REGION_HEADER;
use crate::test_utils::http_client_server_setup;

const DEPRECATED_GATEWAY_INVOKE_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/invoke_gateway_output.json";
const DEPRECATED_GATEWAY_DECLARE_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/declare_gateway_output.json";
const DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_RESPONSE_JSON_PATH: &str =
    "expected_gateway_response/deploy_account_gateway_output.json";

#[rstest]
#[case::invoke(
    GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash!(1_u64))),
    DEPRECATED_GATEWAY_INVOKE_TX_RESPONSE_JSON_PATH,
)]
#[case::declare(
    GatewayOutput::Declare(DeclareGatewayOutput::new(tx_hash!(1_u64), class_hash!(2_u64))),
    DEPRECATED_GATEWAY_DECLARE_TX_RESPONSE_JSON_PATH,

)]
#[case::deploy_account(
    GatewayOutput::DeployAccount(DeployAccountGatewayOutput::new(
        tx_hash!(1_u64),
        contract_address!(3_u64)
    )),
    DEPRECATED_GATEWAY_DEPLOY_ACCOUNT_TX_RESPONSE_JSON_PATH,
)]
#[tokio::test]
async fn gateway_output_json_conversion(
    #[case] gateway_output: GatewayOutput,
    #[case] expected_serialized_response_path: &str,
) {
    let response = Json(gateway_output).into_response();

    let status_code = response.status();
    let response_bytes = &to_bytes(response).await;

    assert_eq!(status_code, StatusCode::OK, "{response_bytes:?}");
    let gateway_response: GatewayOutput = serde_json::from_slice(response_bytes).unwrap();

    let expected_gateway_response =
        serde_json::from_value(read_json_file(expected_serialized_response_path))
            .expect("Failed to deserialize json to GatewayOutput");
    assert_eq!(gateway_response, expected_gateway_response);
}

async fn to_bytes(res: Response) -> Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

#[tokio::test]
async fn error_into_response() {
    let error = HttpServerError::DeserializationError(
        serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err(),
    );
    let response = error.into_response();

    let status = response.status();
    let body = to_bytes(response).await;
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(status.is_success());
    assert_eq!(json.get("code").unwrap(), ErrorCode::ParseError.code());
    assert_eq!(json.get("message").unwrap().as_str().unwrap(), "Failed to parse the request body.");
}

#[traced_test]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, the region of the http request is
/// recorded to the info log.
async fn record_region_test() {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the successful response.
    let tx_hash_1 = TransactionHash(Felt::ONE);
    let tx_hash_2 = TransactionHash(Felt::TWO);
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash_1))));
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(tx_hash_2))));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 1);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server, without a region.
    let rpc_tx = invoke_tx(CairoVersion::default());
    add_tx_http_client.add_tx(rpc_tx).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_1, "N/A").as_str()
    ));

    // Send transaction to the server, with a region.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let region = "test";
    add_tx_http_client.add_tx_with_headers(rpc_tx, [(CLIENT_REGION_HEADER, region)]).await;
    assert!(logs_contain(
        format!("Recorded transaction with hash: {} from region: {}", tx_hash_2, region).as_str()
    ));
}

#[traced_test]
#[tokio::test]
/// Test that when an "add_tx" HTTP request is sent to the server, and it fails in the Gateway, no
/// record of the region is logged.
async fn record_region_gateway_failing_tx() {
    let mut mock_gateway_client = MockGatewayClient::new();
    // Set the failed response.
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::ClientError(ClientError::UnexpectedResponse(
            "mock response".to_string(),
        )),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 2);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    // let http_server_config = HttpServerConfig { ip, port };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Send a transaction to the server.
    let rpc_tx = invoke_tx(CairoVersion::default());
    add_tx_http_client.add_tx(rpc_tx).await;
    assert!(!logs_contain("Recorded transaction with hash: "));
}

#[tokio::test]
async fn test_response() {
    let mut mock_gateway_client = MockGatewayClient::new();

    // Set the successful response.
    let expected_tx_hash = TransactionHash(Felt::ONE);
    mock_gateway_client
        .expect_add_tx()
        .times(1)
        .return_const(Ok(GatewayOutput::Invoke(InvokeGatewayOutput::new(expected_tx_hash))));

    // Set the failed response.
    let expected_error = GatewaySpecError::ClassAlreadyDeclared;
    let expected_err_str = format!(
        "Gateway responded with: {}",
        serde_json::to_string(&ErrorObjectOwned::from(expected_error.clone().into_rpc())).unwrap()
    );
    mock_gateway_client.expect_add_tx().times(1).return_const(Err(
        GatewayClientError::GatewayError(GatewayError::GatewaySpecError {
            source: expected_error,
            p2p_message_metadata: None,
        }),
    ));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 3);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Test a successful response.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let tx_hash = add_tx_http_client.assert_add_tx_success(rpc_tx).await;
    assert_eq!(tx_hash, expected_tx_hash);

    // Test a failed response.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let panicking_task = AssertUnwindSafe(add_tx_http_client.assert_add_tx_success(rpc_tx));
    let error = panicking_task.catch_unwind().await.unwrap_err().downcast::<String>().unwrap();
    let error_str = format!("{}", error);
    assert_eq!(error_str, expected_err_str);
}

const VERSION_1_DEPLOY_ACCOUNT_JSON: &str = r#"{"version": "0x1", "signature": [], "nonce": "0x0", "max_fee": "0x10000000000000000000000000", "class_hash": "0x1", "contract_address_salt": "0x2", "constructor_calldata": [], "type": "DEPRECATED_DEPLOY_ACCOUNT"}"#;
const VERSION_3_INVOKE_JSON: &str = r#"{"version": "0x3", "signature": ["0x1132577", "0x17df53c", "0x0"], "nonce": "0x0", "nonce_data_availability_mode": 0, "fee_data_availability_mode": 0, "resource_bounds": {"L1_GAS": {"max_amount": "0x4000000000000", "max_price_per_unit": "0x4000000000000"}, "L2_GAS": {"max_amount": "0x0", "max_price_per_unit": "0x0"}}, "tip": "0x0", "paymaster_data": [], "sender_address": "0x64", "calldata": ["0x0", "0x1", "0x2", "0x3", "0x4", "0x5", "0x6", "0x7", "0x8", "0x9"], "account_deployment_data": [], "type": "INVOKE_FUNCTION"}"#;
const MODIFIED_VERSION_3_INVOKE_JSON: &str = r#"{"version": "0x3", "signature": ["0x1132577", "0x17df53c", "0x0"], "nonce": "0x0", "nonce_data_availability_mode": 0, "fee_data_availability_mode": 0, "resource_bounds": {"l1_gas": {"max_amount": "0x4000000000000", "max_price_per_unit": "0x4000000000000"}, "l2_gas": {"max_amount": "0x0", "max_price_per_unit": "0x0"}}, "tip": "0x0", "paymaster_data": [], "sender_address": "0x64", "calldata": ["0x0", "0x1", "0x2", "0x3", "0x4", "0x5", "0x6", "0x7", "0x8", "0x9"], "account_deployment_data": [], "type": "INVOKE"}"#;

#[rstest]
#[case::not_a_json(
    "not a json".to_string(),
    "Failed to parse the request body as JSON: expected ident",
    4
)]
#[case::empty_json(
    "{}".to_string(),
    "Failed to deserialize the JSON body into the target type: missing field `type`",
    5
)]
// TODO(Arni): Fix the HTTP server so it behaves differently for this case. The error message should
// be more informative.
#[case::version_1_deploy_account(
    VERSION_1_DEPLOY_ACCOUNT_JSON.to_string(),
    "Failed to deserialize the JSON body into the target type: type: unknown variant \
    `DEPRECATED_DEPLOY_ACCOUNT`, expected one of `DECLARE`, `DEPLOY_ACCOUNT`, `INVOKE`",
    6
)]
#[case::version_3_invoke_function(
    VERSION_3_INVOKE_JSON.to_string(),
    "Failed to deserialize the JSON body into the target type: type: unknown variant \
    `INVOKE_FUNCTION`, expected one of `DECLARE`, `DEPLOY_ACCOUNT`, `INVOKE`",
    7
)]
#[case::formalized_version_3_invoke(
    MODIFIED_VERSION_3_INVOKE_JSON.to_string(),
    "Failed to deserialize the JSON body into the target type: missing field `l1_data_gas`",
    8
)]
#[tokio::test]
async fn malformed_request_body(
    #[case] body: String,
    #[case] expected_response: &str,
    #[case] instance_index: u16,
) {
    let mock_gateway_client = MockGatewayClient::new();

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports =
        AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), instance_index);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Test a failed response.
    // TODO(Arni): Send a request to the URL '/gateway/add_transaction' as well as to
    // '/gateway/add_rpc_transaction'.
    let response = add_tx_http_client.send_raw_request(body).await.text().await.unwrap();

    assert!(
        response.contains(expected_response),
        "Unexpected response; the response is: \n{response}"
    );
}
