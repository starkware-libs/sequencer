use mockito::{Matcher, Mock, Server, ServerGuard};
use reqwest::Url;
use rstest::rstest;
use starknet_api::block::{GasPrice, GasPriceVector, NonzeroGasPrice};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use super::ReplayGasPricesClient;

// Mainnet block 11926844 and its STRK gas prices: the block whose replay deadlocked twice.
const SOURCE_BLOCK_NUMBER: u64 = 11926844;
const L1_GAS_PRICE: u128 = 71204894033059;
const L1_DATA_GAS_PRICE: u128 = 81105312780;
const L2_GAS_PRICE: u128 = 31507218957;

fn tx_hash(seed: u128) -> TransactionHash {
    TransactionHash(Felt::from(seed))
}

fn expected_gas_prices() -> GasPriceVector {
    GasPriceVector {
        l1_gas_price: NonzeroGasPrice::new(GasPrice(L1_GAS_PRICE)).unwrap(),
        l1_data_gas_price: NonzeroGasPrice::new(GasPrice(L1_DATA_GAS_PRICE)).unwrap(),
        l2_gas_price: NonzeroGasPrice::new(GasPrice(L2_GAS_PRICE)).unwrap(),
    }
}

fn tx_metadata_body() -> String {
    format!(r#"{{"timestamp":1783952351,"block_number":{SOURCE_BLOCK_NUMBER}}}"#)
}

// Mirrors what echo_center's `handle_get_block_metadata` emits, hex-encoded as the feeder does.
fn block_metadata_body(l2_gas_price: u128) -> String {
    format!(
        r#"{{"timestamp":1783952351,
            "l1_gas_price_wei":"{l1_wei:#x}","l1_gas_price_fri":"{l1:#x}",
            "l1_data_gas_price_wei":"{data_wei:#x}","l1_data_gas_price_fri":"{data:#x}",
            "l2_gas_price_fri":"{l2:#x}"}}"#,
        l1_wei = 1180777236_u128,
        l1 = L1_GAS_PRICE,
        data_wei = 1344954_u128,
        data = L1_DATA_GAS_PRICE,
        l2 = l2_gas_price,
    )
}

struct Recorder {
    _server: ServerGuard,
    client: ReplayGasPricesClient,
    block_metadata_mock: Option<Mock>,
}

async fn start_recorder(
    base_path: &str,
    tx_metadata: Option<String>,
    block_metadata: Option<(String, u16)>,
) -> Recorder {
    let mut server = Server::new_async().await;
    if let Some(body) = tx_metadata {
        server
            .mock("GET", format!("{base_path}/echonet/get_tx_block_metadata").as_str())
            .match_query(Matcher::Any)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect_at_least(1)
            .create_async()
            .await;
    }
    let block_metadata_mock = match block_metadata {
        Some((body, status)) => Some(
            server
                .mock("GET", format!("{base_path}/echonet/get_block_metadata").as_str())
                .match_query(Matcher::UrlEncoded(
                    "block_number".to_string(),
                    SOURCE_BLOCK_NUMBER.to_string(),
                ))
                .with_status(status.into())
                .with_header("content-type", "application/json")
                .with_body(body)
                .create_async()
                .await,
        ),
        None => None,
    };

    let recorder_url = Url::parse(&format!("{}{base_path}", server.url())).unwrap();
    let client = ReplayGasPricesClient::new(recorder_url);
    Recorder { _server: server, client, block_metadata_mock }
}

#[rstest]
#[case::base_url_without_path("")]
#[case::base_url_with_path("/recorder")]
#[tokio::test]
async fn resolves_the_source_block_gas_prices(#[case] base_path: &str) {
    let recorder = start_recorder(
        base_path,
        Some(tx_metadata_body()),
        Some((block_metadata_body(L2_GAS_PRICE), 200)),
    )
    .await;

    assert_eq!(
        recorder.client.strk_gas_prices(tx_hash(1)).await,
        Some(expected_gas_prices()),
        "all three STRK prices must come from the source block"
    );
}

#[rstest]
#[tokio::test]
async fn memoizes_the_block_lookup() {
    let recorder = start_recorder(
        "",
        Some(tx_metadata_body()),
        Some((block_metadata_body(L2_GAS_PRICE), 200)),
    )
    .await;

    for seed in 1..=3 {
        assert_eq!(
            recorder.client.strk_gas_prices(tx_hash(seed)).await,
            Some(expected_gas_prices())
        );
    }

    recorder.block_metadata_mock.unwrap().expect(1).assert();
}

#[rstest]
#[case::tx_metadata_missing(None, None)]
#[case::unparsable_tx_metadata(Some("not json".to_string()), None)]
#[case::block_metadata_missing(Some(tx_metadata_body()), None)]
#[case::block_metadata_server_error(Some(tx_metadata_body()), Some((String::new(), 500)))]
#[case::unparsable_block_metadata(Some(tx_metadata_body()), Some(("not json".to_string(), 200)))]
#[case::zero_price(Some(tx_metadata_body()), Some((block_metadata_body(0), 200)))]
#[tokio::test]
async fn falls_back_when_the_prices_cannot_be_trusted(
    #[case] tx_metadata: Option<String>,
    #[case] block_metadata: Option<(String, u16)>,
) {
    let recorder = start_recorder("", tx_metadata, block_metadata).await;

    assert_eq!(recorder.client.strk_gas_prices(tx_hash(1)).await, None);
}
