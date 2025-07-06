use alloy::consensus::Header;
use alloy::primitives::B256;
use alloy::providers::mock::Asserter;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header as AlloyRpcHeader};
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::felt;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    Starknet,
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

    let provider = ProviderBuilder::new().on_mocked_client(asserter.clone()).root().clone();
    let contract = Starknet::new(Default::default(), provider);
    let base_layer = EthereumBaseLayerContract { contract, config: Default::default() };

    (base_layer, asserter)
}

#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }
    #[allow(deprecated)] // Legacy code, will be removed soon, don't add new instances if this.
    let (node_handle, starknet_contract_address) = crate::test_utils::get_test_ethereum_node();
    let contract = EthereumBaseLayerContract::new(EthereumBaseLayerConfig {
        node_url: node_handle.0.endpoint().parse().unwrap(),
        starknet_contract_address,
        ..Default::default()
    });

    let first_sn_state_update =
        BlockHashAndNumber { number: BlockNumber(100), hash: BlockHash(felt!("0x100")) };
    let second_sn_state_update =
        BlockHashAndNumber { number: BlockNumber(200), hash: BlockHash(felt!("0x200")) };
    let third_sn_state_update =
        BlockHashAndNumber { number: BlockNumber(300), hash: BlockHash(felt!("0x300")) };

    type Scenario = (u64, Option<BlockHashAndNumber>);
    let scenarios: Vec<Scenario> = vec![
        (0, Some(third_sn_state_update)),
        (5, Some(third_sn_state_update)),
        (15, Some(second_sn_state_update)),
        (25, Some(first_sn_state_update)),
        (1000, None),
    ];
    for (scenario, expected) in scenarios {
        let latest_block = contract.latest_proved_block(scenario).await.unwrap();
        assert_eq!(latest_block, expected);
    }
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

    // Test pectra blob.

    let mocked_block_response =
        &Some(Block::new(AlloyRpcHeader::new(header), BlockTransactions::<B256>::default()));
    asserter.push_success(mocked_block_response);
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();

    assert_eq!(header.base_fee_per_gas, 5);

    // See eip4844::fake_exponential().
    // Roughly e ** (BLOB_GAS / eip7691::BLOB_GASPRICE_UPDATE_FRACTION_PECTRA)
    let expected_pectra_blob_calc = 7;
    assert_eq!(header.blob_fee, expected_pectra_blob_calc);

    // Test legacy blob

    asserter.push_success(mocked_block_response);
    base_layer.config.prague_blob_gas_calc = false;
    let header = base_layer.get_block_header(0).await.unwrap().unwrap();
    // Roughly e ** (BLOB_GAS / eip4844::BLOB_GASPRICE_UPDATE_FRACTION)
    let expected_original_blob_calc = 19;
    assert_eq!(header.blob_fee, expected_original_blob_calc);
}
