use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::core::EntryPointSelector;
use starknet_api::transaction::L1HandlerTransaction;
use starknet_api::{calldata, contract_address, felt};

use crate::constants::{EventIdentifier, LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER};
use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    L1ToL2MessageArgs,
    Starknet,
};
use crate::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
    get_test_ethereum_node,
    DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS,
};
use crate::{BaseLayerContract, L1Event};

fn in_ci() -> bool {
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

    // Test: Get events from L1 contract and other instances of this L1 contract.
    // Setup.

    // Deploy the contract to the anvil instance.
    Starknet::deploy(this_contract.contract.provider().clone()).await.unwrap();
    // Deploy another instance of the contract to the same anvil instance.
    let other_contract = Starknet::deploy(this_contract.contract.provider().clone()).await.unwrap();
    assert_ne!(
        this_contract.contract.address(),
        other_contract.address(),
        "The two contracts should be different."
    );

    let this_l1_handler = L1HandlerTransaction {
        contract_address: contract_address!("0x12"),
        entry_point_selector: EntryPointSelector(felt!("0x34")),
        calldata: calldata!(DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, felt!("0x1"), felt!("0x2")),
        ..Default::default()
    };
    let this_receipt = this_contract
        .contract
        .send_message_to_l2(&L1ToL2MessageArgs { tx: this_l1_handler.clone(), l1_tx_nonce: 2 })
        .await;
    assert!(this_receipt.status());
    let this_block_number = this_receipt.block_number.unwrap();

    let other_l1_handler = L1HandlerTransaction {
        contract_address: contract_address!("0x56"),
        entry_point_selector: EntryPointSelector(felt!("0x78")),
        calldata: calldata!(DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, felt!("0x1"), felt!("0x2")),
        ..Default::default()
    };
    let other_receipt = other_contract
        .send_message_to_l2(&L1ToL2MessageArgs { tx: other_l1_handler.clone(), l1_tx_nonce: 3 })
        .await;
    assert!(other_receipt.status());
    let other_block_number = other_receipt.block_number.unwrap();

    let min_block_number = this_block_number.min(other_block_number).saturating_sub(1);
    let max_block_number = this_block_number.max(other_block_number).saturating_add(1);

    // Test the events.
    let mut events =
        this_contract.events(min_block_number..=max_block_number, EVENT_IDENTIFIERS).await.unwrap();

    assert_eq!(events.len(), 1, "Expected only events from this contract.");
    assert_matches!(events.remove(0), L1Event::LogMessageToL2 { tx, .. } if tx == this_l1_handler);
}
