use alloy_primitives::Address as EthereumContractAddress;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::felt;
use url::Url;

use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::test_utils::{anvil, get_test_ethereum_node};
use crate::BaseLayerContract;

// TODO(Gilad): move to global test_utils crate and use everywhere instead of relying on the
// confusing `#[ignore]` api to mark slow tests.
fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

fn ethereum_base_layer_contract(
    node_url: Url,
    starknet_contract_address: EthereumContractAddress,
) -> EthereumBaseLayerContract {
    let config = EthereumBaseLayerConfig { node_url, starknet_contract_address };
    EthereumBaseLayerContract::new(config)
}

#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }

    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
    let node_url = node_handle.0.endpoint().parse().unwrap();
    let contract = ethereum_base_layer_contract(node_url, starknet_contract_address);

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
async fn get_proved_block_at_unknown_block_number() {
    if !in_ci() {
        return;
    }

    // TODO(Gilad): Moved into test-utils in an upcoming PR.
    const DEFAULT_ANVIL_DEPLOY_ADDRESS: &str = "0x5fbdb2315678afecb367f032d93f642f64180aa3";

    let anvil = anvil();
    let node_url = anvil.endpoint_url();
    let starknet_contract_address = DEFAULT_ANVIL_DEPLOY_ADDRESS.parse().unwrap();
    let contract = ethereum_base_layer_contract(node_url, starknet_contract_address);

    assert!(
        contract
            .get_proved_block_at(123)
            .await
            .unwrap_err()
            // This error is nested way too deep inside `alloy`.
            .to_string()
            .contains("BlockOutOfRangeError")
    );
}

#[tokio::test]
async fn get_gas_price_and_timestamps() {
    // if !in_ci() {
    //     return;
    // }

    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
    let node_url = node_handle.0.endpoint().parse().unwrap();
    let contract = ethereum_base_layer_contract(node_url, starknet_contract_address);

    let block_number = 30;
    let gas_price = contract.get_block_gas_price(block_number).await.unwrap().unwrap();

    // TODO(guyn): Figure out why the data gas is None.
    let data_gas_price = contract.get_block_data_gas_price(block_number).await.unwrap();

    let timestamp = contract.get_block_timestamp(block_number).await.unwrap().unwrap();

    // TODO(guyn): Figure out how these numbers are calculated, instead of just printing and testing
    // against what we got. Use this println! to get the numbers to put into the asserts.
    println!(
        "Gas price: {}, data gas price: {:?}, timestamp: {}",
        gas_price, data_gas_price, timestamp
    );
    assert_eq!(gas_price, 20168195);
    assert_eq!(data_gas_price, None);
    assert_eq!(timestamp, 1676992456);

    let price_sample = contract.get_price_sample(block_number).await.unwrap().unwrap();
    assert_eq!(price_sample.timestamp, timestamp);
    assert_eq!(price_sample.base_fee_per_gas, gas_price);
    assert_eq!(price_sample.blob_fee, 0);
}
