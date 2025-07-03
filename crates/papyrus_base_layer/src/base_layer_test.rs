use alloy::consensus::Header;
use alloy::primitives::B256;
use alloy::providers::mock::Asserter;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header as AlloyRpcHeader};
use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::core::EntryPointSelector;
use starknet_api::transaction::L1HandlerTransaction;
use starknet_api::{calldata, contract_address, felt};

use crate::anvil_base_layer::AnvilBaseLayer;
use crate::constants::{EventIdentifier, LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER};
use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    L1ToL2MessageArgs,
    Starknet,
};
use crate::test_utils::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS;
use crate::{BaseLayerContract, L1Event};

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
    let this_contract = anvil_base_layer.ethereum_base_layer;

    // Setup.

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
