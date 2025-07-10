use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::ParamPrivacyInput;
use assert_matches::assert_matches;
use mockito::{Matcher, Server, ServerGuard};
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::{
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::felt;

use super::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClient,
    PreconfirmedCendeClientError,
    PreconfirmedCendeClientTrait,
    PreconfirmedCendeConfig,
    RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH,
};
use crate::cende_client_types::{CendeBlockMetadata, CendePreconfirmedBlock};

const TEST_BLOCK_NUMBER: BlockNumber = BlockNumber(123);
const TEST_ROUND: Round = 1;
const TEST_WRITE_ITERATION: u64 = 2;
const TEST_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234567890);
const TEST_GAS_PRICE: GasPrice = GasPrice(1);
const TEST_SEQUENCER_ADDRESS: &str = "0x111";
const TEST_RECORDER_URL: &str = "https://test.com";
const TEST_EXAMPLE_URL: &str = "https://example.com";

fn test_cende_client(server: &mut ServerGuard) -> PreconfirmedCendeClient {
    let config = PreconfirmedCendeConfig { recorder_url: server.url().parse().unwrap() };
    PreconfirmedCendeClient::new(config)
}

fn test_preconfirmed_block_data() -> CendeWritePreconfirmedBlock {
    CendeWritePreconfirmedBlock {
        block_number: TEST_BLOCK_NUMBER,
        round: TEST_ROUND,
        write_iteration: TEST_WRITE_ITERATION,
        pre_confirmed_block: CendePreconfirmedBlock {
            metadata: CendeBlockMetadata {
                status: "PENDING",
                starknet_version: StarknetVersion::default(),
                l1_da_mode: L1DataAvailabilityMode::Calldata,
                l1_gas_price: GasPricePerToken {
                    price_in_fri: TEST_GAS_PRICE,
                    price_in_wei: TEST_GAS_PRICE,
                },
                l1_data_gas_price: GasPricePerToken {
                    price_in_fri: TEST_GAS_PRICE,
                    price_in_wei: TEST_GAS_PRICE,
                },
                l2_gas_price: GasPricePerToken {
                    price_in_fri: TEST_GAS_PRICE,
                    price_in_wei: TEST_GAS_PRICE,
                },
                timestamp: TEST_TIMESTAMP,
                sequencer_address: ContractAddress::try_from(felt!(TEST_SEQUENCER_ADDRESS)).unwrap(),
            },
            transactions: vec![],
            transaction_receipts: vec![],
            transaction_state_diffs: vec![],
        },
    }
}

async fn create_mock_server() -> ServerGuard {
    Server::new_async().await
}

#[test]
fn test_new_client() {
    let recorder_url = TEST_EXAMPLE_URL.parse().unwrap();
    let config = PreconfirmedCendeConfig { recorder_url };
    let _client = PreconfirmedCendeClient::new(config);

    // We can't access the private field directly, but we can test that the constructor works
    // The actual URL construction will be tested through the mock HTTP calls
}

#[test]
fn test_config_default() {
    let config = PreconfirmedCendeConfig::default();
    assert_eq!(config.recorder_url.as_str(), "https://recorder_url/");
}

#[test]
fn test_config_serialization() {
    let config = PreconfirmedCendeConfig { recorder_url: TEST_RECORDER_URL.parse().unwrap() };

    let serialized = config.dump();

    let expected = BTreeMap::from([ser_param(
        "recorder_url",
        &config.recorder_url,
        "The URL of the Pythonic cende_recorder",
        ParamPrivacyInput::Private,
    )]);

    assert_eq!(serialized, expected);
}

#[tokio::test]
async fn test_write_pre_confirmed_block_success() {
    let mut server = create_mock_server().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(200)
        .with_body("")
        .create_async()
        .await;

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_write_pre_confirmed_block_error_response() {
    let mut server = create_mock_server().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(400)
        .with_body("Bad Request")
        .create_async()
        .await;

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
    assert!(result.is_err());

    let error_msg = assert_matches!(
        result.unwrap_err(),
        PreconfirmedCendeClientError::CendeRecorderError(msg) => msg
    );
    assert!(error_msg.contains("write_pre_confirmed_block request failed"));
    assert!(error_msg.contains(&format!("block_number: {}", TEST_BLOCK_NUMBER.0)));
    assert!(error_msg.contains(&format!("round: {}", TEST_ROUND)));
    assert!(error_msg.contains(&format!("write_iteration: {}", TEST_WRITE_ITERATION)));
    assert!(error_msg.contains("status: 400"));
}

#[tokio::test]
async fn test_write_pre_confirmed_block_server_error() {
    let mut server = create_mock_server().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(500)
        .with_body("Internal Server Error")
        .create_async()
        .await;

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
    assert!(result.is_err());

    let error_msg = assert_matches!(
        result.unwrap_err(),
        PreconfirmedCendeClientError::CendeRecorderError(msg) => msg
    );
    assert!(error_msg.contains("status: 500"));
}

#[tokio::test]
async fn test_write_pre_confirmed_block_request_serialization() {
    let mut server = create_mock_server().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    // Mock that expects JSON body
    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(200)
        .match_body(Matcher::Json(serde_json::json!({
            "block_number": TEST_BLOCK_NUMBER.0,
            "round": TEST_ROUND,
            "write_iteration": TEST_WRITE_ITERATION,
            "pre_confirmed_block": {
                "status": "PENDING",
                "starknet_version": "0.14.1",
                "l1_da_mode": "CALLDATA",
                "l1_gas_price": {
                    "price_in_fri": "0x1",
                    "price_in_wei": "0x1"
                },
                "l1_data_gas_price": {
                    "price_in_fri": "0x1",
                    "price_in_wei": "0x1"
                },
                "l2_gas_price": {
                    "price_in_fri": "0x1",
                    "price_in_wei": "0x1"
                },
                "timestamp": TEST_TIMESTAMP.0,
                "sequencer_address": TEST_SEQUENCER_ADDRESS,
                "transactions": [],
                "transaction_receipts": [],
                "transaction_state_diffs": []
            }
        })))
        .create_async()
        .await;

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
    assert!(result.is_ok());
}

#[rstest]
#[case(200, true)]
#[case(201, true)]
#[case(204, true)]
#[case(400, false)]
#[case(401, false)]
#[case(404, false)]
#[case(500, false)]
#[tokio::test]
async fn test_write_pre_confirmed_block_different_status_codes(
    #[case] status_code: usize,
    #[case] should_succeed: bool,
) {
    let mut server = create_mock_server().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(status_code)
        .with_body("")
        .create_async()
        .await;

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;

    if should_succeed {
        assert!(result.is_ok(), "Status code {} should succeed", status_code);
    } else {
        assert!(result.is_err(), "Status code {} should fail", status_code);
    }
}

#[tokio::test]
async fn test_write_pre_confirmed_block_network_error() {
    // Create a client with an invalid URL to simulate network error
    let config = PreconfirmedCendeConfig {
        recorder_url: "http://invalid-url-that-should-not-exist.com".parse().unwrap(),
    };
    let client = PreconfirmedCendeClient::new(config);
    let test_data = test_preconfirmed_block_data();

    let result = client.write_pre_confirmed_block(test_data).await;

    assert!(result.is_err());
    assert_matches!(
        result.unwrap_err(),
        PreconfirmedCendeClientError::RequestError(_)
    );
}

#[test]
fn test_error_types() {
    let recorder_error = PreconfirmedCendeClientError::CendeRecorderError("test error".to_string());

    // Test Display trait
    assert_eq!(recorder_error.to_string(), "CendeRecorder returned an error: test error");

    // Test Error trait
    assert!(std::error::Error::source(&recorder_error).is_none());
}

#[test]
fn test_url_construction() {
    let base_url = &format!("{}/base", TEST_EXAMPLE_URL);
    let config = PreconfirmedCendeConfig { recorder_url: base_url.parse().unwrap() };
    let _client = PreconfirmedCendeClient::new(config);

    // We can't access the private field directly, but we can test that the constructor works
    // The actual URL construction will be tested through the mock HTTP calls
}

#[tokio::test]
async fn test_concurrent_requests() {
    let mut server = create_mock_server().await;

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(200)
        .with_body("")
        .expect_at_least(3)
        .create_async()
        .await;

    let mut handles = vec![];
    for i in 0..3 {
        let client = test_cende_client(&mut server);
        let mut test_data = test_preconfirmed_block_data();
        test_data.write_iteration = i;

        let handle = tokio::spawn(async move { client.write_pre_confirmed_block(test_data).await });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    mock_response.assert_async().await;
}
