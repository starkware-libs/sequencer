use alloy::consensus::Header;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::mock::Asserter;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header as AlloyRpcHeader};
use pretty_assertions::assert_eq;
use url::Url;

use crate::eth_events::{create_l1_event_data, felt_max_u256, u256_exceeds_felt};
use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumBaseLayerError,
};
use crate::BaseLayerContract;

// TODO(Gilad): Use everywhere instead of relying on the confusing `#[ignore]` api to mark slow
// tests.
pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

fn base_layer_with_mocked_provider() -> (EthereumBaseLayerContract, Asserter) {
    // See alloy docs, functions as a queue of mocked responses, success or failure.
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone()).root().clone();
    let config = EthereumBaseLayerConfig::default();
    let base_layer = EthereumBaseLayerContract::new_with_provider(config, provider);

    (base_layer, asserter)
}

#[tokio::test]
async fn get_gas_price_and_timestamps() {
    if !in_ci() {
        return;
    }
    // Setup.
    let (mut base_layer, asserter) = base_layer_with_mocked_provider();

    // Selected in order to make the blob calc below non trivial.
    const BLOB_GAS: u128 = 10000000;

    let header = Header {
        base_fee_per_gas: Some(5),
        excess_blob_gas: Some(BLOB_GAS.try_into().unwrap()),
        ..Default::default()
    };

    let mocked_block_response =
        &Some(Block::new(AlloyRpcHeader::new(header), BlockTransactions::<B256>::default()));

    // Test fusaka blob.
    base_layer.config.fusaka_no_bpo_start_block_number = 0;
    base_layer.config.bpo1_start_block_number = 10;
    base_layer.config.bpo2_start_block_number = 10;

    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    assert_eq!(header.base_fee_per_gas, 5);

    // See eip4844::fake_exponential().
    // Roughly e ** (BLOB_GAS / eip7691::BLOB_GASPRICE_UPDATE_FRACTION_PECTRA)
    let expected_fusaka_blob_calc = 7;
    assert_eq!(header.blob_fee, expected_fusaka_blob_calc);

    // Test BPO1 blob.
    base_layer.config.fusaka_no_bpo_start_block_number = 0;
    base_layer.config.bpo1_start_block_number = 0;
    base_layer.config.bpo2_start_block_number = 10;

    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    // See eip4844::fake_exponential().
    // Roughly e ** (BLOB_GAS / eip7691::BLOB_GASPRICE_UPDATE_FRACTION_PECTRA)
    let expected_bpo1_blob_calc = 3;
    assert_eq!(header.blob_fee, expected_bpo1_blob_calc);

    // Test BPO2 blob.
    base_layer.config.fusaka_no_bpo_start_block_number = 0;
    base_layer.config.bpo1_start_block_number = 0;
    base_layer.config.bpo2_start_block_number = 0;

    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    let expected_bpo2_blob_calc = 2;
    assert_eq!(header.blob_fee, expected_bpo2_blob_calc);

    // Test pectra blob.
    base_layer.config.fusaka_no_bpo_start_block_number = 10;
    base_layer.config.bpo1_start_block_number = 10;
    base_layer.config.bpo2_start_block_number = 10;

    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    // See eip4844::fake_exponential().
    // Roughly e ** (BLOB_GAS / eip7691::BLOB_GASPRICE_UPDATE_FRACTION_PECTRA)
    let expected_pectra_blob_calc = 7;
    assert_eq!(header.blob_fee, expected_pectra_blob_calc);
}

#[tokio::test]
async fn test_cycle_wraps_to_primary_through_full_list() {
    let primary_url = Url::parse("http://primary-endpoint.test/").unwrap();
    let secondary_url = Url::parse("http://secondary-endpoint.test/").unwrap();
    let tertiary_url = Url::parse("http://tertiary-endpoint.test/").unwrap();
    let config = EthereumBaseLayerConfig {
        ordered_l1_endpoint_urls: vec![
            primary_url.clone().into(),
            secondary_url.clone().into(),
            tertiary_url.clone().into(),
        ],
        ..Default::default()
    };
    let mut base_layer = EthereumBaseLayerContract::new(config);

    // The live provider starts on the primary (first) endpoint.
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), primary_url);

    // First cycle moves to the secondary endpoint.
    base_layer.cycle_provider_url().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), secondary_url);

    // Second cycle moves to the tertiary endpoint.
    base_layer.cycle_provider_url().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), tertiary_url);

    // Third cycle wraps back to the primary endpoint.
    base_layer.cycle_provider_url().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), primary_url);
}

#[tokio::test]
async fn test_reset_provider_url_to_primary_repoints_live_provider() {
    let primary_url = Url::parse("http://primary-endpoint.test/").unwrap();
    let secondary_url = Url::parse("http://secondary-endpoint.test/").unwrap();
    let tertiary_url = Url::parse("http://tertiary-endpoint.test/").unwrap();
    let config = EthereumBaseLayerConfig {
        ordered_l1_endpoint_urls: vec![
            primary_url.clone().into(),
            secondary_url.clone().into(),
            tertiary_url.clone().into(),
        ],
        ..Default::default()
    };
    let mut base_layer = EthereumBaseLayerContract::new(config);

    // Cycle twice to land on the tertiary endpoint.
    base_layer.cycle_provider_url().await.unwrap();
    base_layer.cycle_provider_url().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), tertiary_url);

    // Reset to primary.
    base_layer.reset_provider_url_to_primary().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), primary_url);

    // Calling reset again when already on primary is a no-op.
    base_layer.reset_provider_url_to_primary().await.unwrap();
    assert_eq!(base_layer.get_url().await.unwrap().expose_secret(), primary_url);
}

#[test]
fn create_l1_event_data_rejects_out_of_range_inputs() {
    let oversized = felt_max_u256() + U256::from(1_u8);
    assert!(u256_exceeds_felt(oversized));

    let cases = [
        ("to_address", oversized, U256::from(1_u8), U256::from(1_u8), vec![U256::from(1_u8)]),
        ("selector", U256::from(1_u8), oversized, U256::from(1_u8), vec![U256::from(1_u8)]),
        ("nonce", U256::from(1_u8), U256::from(1_u8), oversized, vec![U256::from(1_u8)]),
        ("payload", U256::from(1_u8), U256::from(1_u8), U256::from(1_u8), vec![oversized]),
    ];

    for (field, to_address, selector, nonce, payload) in cases {
        let result = create_l1_event_data(Address::ZERO, to_address, selector, &payload, nonce);
        assert_eq!(
            result,
            Err(EthereumBaseLayerError::CalldataValueOutOfRange(oversized)),
            "Unexpected result for field: {field}"
        );
    }
}

#[test]
fn test_u256_exceeds_felt_for_extreme_values() {
    // Felt(-1) should map to FIELD_PRIME - 1 and be accepted.
    let felt_neg_one = felt_max_u256();
    assert!(!u256_exceeds_felt(felt_neg_one));

    assert!(u256_exceeds_felt(felt_neg_one + U256::from(1_u8)));
}
