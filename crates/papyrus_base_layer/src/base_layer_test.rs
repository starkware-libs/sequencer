use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::felt;

use crate::constants::{EventIdentifier, LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER};
use crate::ethereum_base_layer_contract::{EthereumBaseLayerConfig, EthereumBaseLayerContract};
use crate::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
    get_test_ethereum_node,
    DEFAULT_ANVIL_ADDITIONAL_ADDRESS_INDEX,
};
use crate::{BaseLayerContract, L1Event};

// TODO(Gilad): Use everywhere instead of relying on the confusing `#[ignore]` api to mark slow
// tests.
pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

#[tokio::test]
// Note: the test requires ganache-cli installed, otherwise it is ignored.
async fn latest_proved_block_ethereum() {
    if !in_ci() {
        return;
    }

    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
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

    let (node_handle, starknet_contract_address) = get_test_ethereum_node();
    let contract = EthereumBaseLayerContract::new(EthereumBaseLayerConfig {
        node_url: node_handle.0.endpoint().parse().unwrap(),
        starknet_contract_address,
        ..Default::default()
    });

    let block_number = 30;
    let price_sample = contract.get_price_sample(block_number).await.unwrap().unwrap();

    // TODO(guyn): Figure out how these numbers are calculated, instead of just printing and testing
    // against what we got.
    assert_eq!(price_sample.timestamp, 1676992456);
    assert_eq!(price_sample.base_fee_per_gas, 20168195);
    assert_eq!(price_sample.blob_fee, 0);
}

#[tokio::test]
async fn events_from_other_contract() {
    if !in_ci() {
        return;
    }
    const EVENT_IDENTIFIERS: &[EventIdentifier] = &[LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER];

    let this_config = ethereum_base_layer_config_for_anvil(None);
    let _anvil = anvil_instance_from_config(&this_config);
    let this_contract = EthereumBaseLayerContract::new(this_config.clone());

    // Test: get_proved_block_at_unknown_block_number.
    // TODO(Arni): turn this into a unit test, with its own anvil instance.
    assert!(
        this_contract
            .get_proved_block_at(123)
            .await
            .unwrap_err()
            // This error is nested way too deep inside `alloy`.
            .to_string()
            .contains("BlockOutOfRangeError")
    );

    let other_address = _anvil.addresses()[DEFAULT_ANVIL_ADDITIONAL_ADDRESS_INDEX];

    assert_ne!(
        this_config.starknet_contract_address, other_address,
        "The two contracts should be different, otherwise this test is pointless."
    );

    let other_config =
        EthereumBaseLayerConfig { starknet_contract_address: other_address, ..this_config };
    let other_contract = EthereumBaseLayerContract::new(other_config);

    let l2_contract_address = "0x12";
    let l2_entry_point = "0x34";
    let message_to_this_contract = this_contract.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![],
    );
    message_to_this_contract.send().await.unwrap().get_receipt().await.unwrap();

    let other_l2_contract_address = "0x56";
    let other_l2_entry_point = "0x78";
    let message_to_other_contract = other_contract.contract.sendMessageToL2(
        other_l2_contract_address.parse().unwrap(),
        other_l2_entry_point.parse().unwrap(),
        vec![],
    );
    message_to_other_contract.send().await.unwrap().get_receipt().await.unwrap();

    let events = this_contract.events(0..=100, EVENT_IDENTIFIERS).await.unwrap();
    // TODO(Arni): Fix this test. Make it so just one event is returned.
    assert_eq!(
        events.len(),
        2,
        "Expected both events to be present even though one of them was sent to a different \
         contract."
    );
    let _tx = assert_matches!(events.first().unwrap(), L1Event::LogMessageToL2 { tx, .. } => tx);
    // assert_eq!(tx.contract_address, starknet_api::contract_address!(l2_contract_address));
}
