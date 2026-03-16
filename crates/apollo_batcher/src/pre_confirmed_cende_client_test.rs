use apollo_batcher_types::batcher_types::Round;
use metrics_exporter_prometheus::PrometheusBuilder;
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
use url::Url;

use super::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClient,
    PreconfirmedCendeClientTrait,
    PreconfirmedCendeConfig,
    RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH,
};
use crate::cende_client_types::{CendeBlockMetadata, CendePreconfirmedBlock};
use crate::metrics::{
    PreconfirmedBlockWriteFailureReason,
    LABEL_NAME_PRECONFIRMED_BLOCK_WRITE_FAILURE_REASON,
    PRECONFIRMED_BLOCK_WRITE_FAILURE,
};

const TEST_BLOCK_NUMBER: BlockNumber = BlockNumber(123);
const TEST_ROUND: Round = 1;
const TEST_WRITE_ITERATION: u64 = 2;
const TEST_TIMESTAMP: BlockTimestamp = BlockTimestamp(1234567890);
const TEST_GAS_PRICE: GasPrice = GasPrice(1);
const TEST_SEQUENCER_ADDRESS: &str = "0x111";

fn test_cende_client(server: &mut ServerGuard) -> PreconfirmedCendeClient {
    let config = PreconfirmedCendeConfig { recorder_url: server.url().parse::<Url>().unwrap() };
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

    client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;
}

#[tokio::test]
async fn test_write_pre_confirmed_block_error_response() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    PRECONFIRMED_BLOCK_WRITE_FAILURE.register();

    let mut server = Server::new_async().await;
    let client = test_cende_client(&mut server);
    let test_data = test_preconfirmed_block_data();

    let mock_response = server
        .mock("POST", RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
        .with_status(400)
        .with_body("Bad Request")
        .create();

    client.write_pre_confirmed_block(test_data).await;

    mock_response.assert_async().await;

    let metrics = recorder.handle().render();
    PRECONFIRMED_BLOCK_WRITE_FAILURE.assert_eq::<u64>(
        &metrics,
        1,
        &[(
            LABEL_NAME_PRECONFIRMED_BLOCK_WRITE_FAILURE_REASON,
            PreconfirmedBlockWriteFailureReason::HttpClientError.into(),
        )],
    );
}

#[tokio::test]
async fn test_write_pre_confirmed_block_send_error_metric() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    PRECONFIRMED_BLOCK_WRITE_FAILURE.register();

    let config =
        PreconfirmedCendeConfig { recorder_url: "http://127.0.0.1:1".parse::<Url>().unwrap() };
    let client = PreconfirmedCendeClient::new(config);
    let test_data = test_preconfirmed_block_data();

    client.write_pre_confirmed_block(test_data).await;

    let metrics = recorder.handle().render();
    PRECONFIRMED_BLOCK_WRITE_FAILURE.assert_eq::<u64>(
        &metrics,
        1,
        &[(
            LABEL_NAME_PRECONFIRMED_BLOCK_WRITE_FAILURE_REASON,
            PreconfirmedBlockWriteFailureReason::SendConnect.into(),
        )],
    );
}
