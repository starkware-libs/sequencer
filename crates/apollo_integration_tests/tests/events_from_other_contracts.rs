use apollo_integration_tests::anvil_base_layer::{send_message_to_l2, AnvilBaseLayer};
use assert_matches::assert_matches;
use papyrus_base_layer::constants::{EventIdentifier, LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER};
use papyrus_base_layer::ethereum_base_layer_contract::Starknet;
use papyrus_base_layer::{BaseLayerContract, L1Event};
use pretty_assertions::assert_eq;
use starknet_api::core::EntryPointSelector;
use starknet_api::transaction::L1HandlerTransaction;
use starknet_api::{calldata, contract_address, felt};

pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

// Ensure that the base layer instance filters out events from other deployments of the core
// contract.
#[tokio::test]
async fn events_from_other_contract() {
    if !in_ci() {
        return;
    }
    const EVENT_IDENTIFIERS: &[EventIdentifier] = &[LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER];

    let anvil_base_layer = AnvilBaseLayer::new().await;
    // Anvil base layer already auto-deployed a starknet contract.
    let this_contract = &anvil_base_layer.ethereum_base_layer.contract;

    // Setup.

    // Deploy another instance of the contract to the same anvil instance.
    let other_contract = Starknet::deploy(this_contract.provider().clone()).await.unwrap();
    assert_ne!(
        this_contract.address(),
        other_contract.address(),
        "The two contracts should be different."
    );

    let this_l1_handler = L1HandlerTransaction {
        contract_address: contract_address!("0x12"),
        entry_point_selector: EntryPointSelector(felt!("0x34")),
        calldata: calldata!(
            AnvilBaseLayer::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS,
            felt!("0x1"),
            felt!("0x2")
        ),
        ..Default::default()
    };
    let this_receipt = send_message_to_l2(this_contract, &this_l1_handler.clone()).await;
    assert!(this_receipt.status());
    let this_block_number = this_receipt.block_number.unwrap();

    let other_l1_handler = L1HandlerTransaction {
        contract_address: contract_address!("0x56"),
        entry_point_selector: EntryPointSelector(felt!("0x78")),
        calldata: calldata!(
            AnvilBaseLayer::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS,
            felt!("0x1"),
            felt!("0x2")
        ),
        ..Default::default()
    };
    let other_receipt = send_message_to_l2(&other_contract, &other_l1_handler.clone()).await;
    assert!(other_receipt.status());
    let other_block_number = other_receipt.block_number.unwrap();

    let min_block_number = this_block_number.min(other_block_number).saturating_sub(1);
    let max_block_number = this_block_number.max(other_block_number).saturating_add(1);

    // Test the events.
    let mut events = anvil_base_layer
        .ethereum_base_layer
        .events(min_block_number..=max_block_number, EVENT_IDENTIFIERS)
        .await
        .unwrap();

    assert_eq!(events.len(), 1, "Expected only events from this contract.");
    assert_matches!(events.remove(0), L1Event::LogMessageToL2 { tx, .. } if tx == this_l1_handler);
}
