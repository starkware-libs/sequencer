use mempool_test_utils::in_ci;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::felt;

use crate::test_utils::EthereumBaseLayerContractBuilder;
use crate::BaseLayerContract;

#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }

    let (_node_handle, contract) = EthereumBaseLayerContractBuilder::new_for_ganache().build();

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

    let (_node_handle, contract) = EthereumBaseLayerContractBuilder::new_for_anvil().build();

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
    if !in_ci() {
        return;
    }

    let (_node_handle, contract) = EthereumBaseLayerContractBuilder::new_for_ganache().build();

    let block_number = 30;
    let price_sample = contract.get_price_sample(block_number).await.unwrap().unwrap();

    // TODO(guyn): Figure out how these numbers are calculated, instead of just printing and testing
    // against what we got.
    assert_eq!(price_sample.timestamp, 1676992456);
    assert_eq!(price_sample.base_fee_per_gas, 20168195);
    assert_eq!(price_sample.blob_fee, 0);
}
