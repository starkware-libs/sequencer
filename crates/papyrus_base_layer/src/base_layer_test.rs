use alloy::consensus::Header;
use alloy::primitives::B256;
use alloy::providers::mock::Asserter;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header as AlloyRpcHeader};
use pretty_assertions::assert_eq;

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
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
