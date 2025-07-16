use apollo_batcher_types::batcher_types::Round;
use assert_matches::assert_matches;
use mockito::{Server, ServerGuard};
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
                sequencer_address: ContractAddress::try_from(felt!(TEST_SEQUENCER_ADDRESS))
                    .unwrap(),
            },
            transactions: vec![],
            transaction_receipts: vec![],
            transaction_state_diffs: vec![],
        },
    }
}

#[tokio::test]
async fn test_write_pre_confirmed_block_success() {
    let mut server = Server::new_async().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(200)
        .with_body("")
        .create();

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_write_pre_confirmed_block_error_response() {
    let mut server = Server::new_async().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(400)
        .with_body("Bad Request")
        .create();

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert();
    let error_msg = assert_matches!(
        result.unwrap_err(),
        PreconfirmedCendeClientError::CendeRecorderError(msg) => msg
    );
    assert!(error_msg.contains("write_pre_confirmed_block request failed"));
    assert!(error_msg.contains(&format!("block_number={}", TEST_BLOCK_NUMBER.0)));
    assert!(error_msg.contains(&format!("round={TEST_ROUND}")));
    assert!(error_msg.contains(&format!("write_iteration={TEST_WRITE_ITERATION}")));
    assert!(error_msg.contains("status=400"));
}

#[tokio::test]
async fn test_write_pre_confirmed_block_server_error() {
    let mut server = Server::new_async().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(500)
        .with_body("Internal Server Error")
        .create();

    let result = client.write_pre_confirmed_block(test_data).await;

    mock_response.assert();
    let error_msg = assert_matches!(
        result.unwrap_err(),
        PreconfirmedCendeClientError::CendeRecorderError(msg) => msg
    );
    assert!(error_msg.contains("status=500"));
}

#[tokio::test]
async fn test_write_pre_confirmed_block_network_error() {
    let config = PreconfirmedCendeConfig {
        recorder_url: "http://invalid-url-that-should-not-exist.pmrewpohg".parse().unwrap(),
    };
    let client = PreconfirmedCendeClient::new(config);
    let test_data = test_preconfirmed_block_data();
    let result = client.write_pre_confirmed_block(test_data).await;
    let PreconfirmedCendeClientError::RequestError(e) = result.unwrap_err() else {
        panic!("Incorrect error type.")
    };
    assert!(e.is_connect());
}
