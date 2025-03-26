use std::sync::Arc;

use alloy::primitives::U256;
use apollo_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    Starknet,
};
use apollo_base_layer::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
};
use mempool_test_utils::in_ci;
use mempool_test_utils::starknet_api_test_utils::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS;
use starknet_api::contract_address;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{L1HandlerTransaction, TransactionHasher, TransactionVersion};
use starknet_l1_provider_types::Event;

use crate::event_identifiers_to_track;
use crate::l1_scraper::{L1Scraper, L1ScraperConfig};
use crate::test_utils::FakeL1ProviderClient;

// TODO(Gilad): Replace EthereumBaseLayerContract with a mock that has a provider initialized with
// `with_recommended_fillers`, in order to be able to create txs from non-default users.
async fn scraper(
    base_layer_config: EthereumBaseLayerConfig,
) -> (L1Scraper<EthereumBaseLayerContract>, Arc<FakeL1ProviderClient>) {
    let fake_client = Arc::new(FakeL1ProviderClient::default());
    let base_layer = EthereumBaseLayerContract::new(base_layer_config);

    // Deploy a fresh Starknet contract on Anvil from the bytecode in the JSON file.
    Starknet::deploy(base_layer.contract.provider().clone()).await.unwrap();

    let scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        fake_client.clone(),
        base_layer,
        event_identifiers_to_track(),
    )
    .await
    .unwrap();

    (scraper, fake_client)
}

#[tokio::test]
// TODO(Gilad): extract setup stuff into test helpers once more tests are added and patterns emerge.
async fn txs_happy_flow() {
    if !in_ci() {
        return;
    }

    let base_layer_config = ethereum_base_layer_config_for_anvil(None);
    let _anvil = anvil_instance_from_config(&base_layer_config);
    // Setup.
    let (mut scraper, fake_client) = scraper(base_layer_config).await;

    // Test.
    // Scrape multiple events.
    let l2_contract_address = "0x12";
    let l2_entry_point = "0x34";

    let message_to_l2_0 = scraper.base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(1_u8), U256::from(2_u8)],
    );
    let message_to_l2_1 = scraper.base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(3_u8), U256::from(4_u8)],
    );

    // Send the transactions.
    for msg in &[message_to_l2_0, message_to_l2_1] {
        msg.send().await.unwrap().get_receipt().await.unwrap();
    }

    const EXPECTED_VERSION: TransactionVersion = TransactionVersion(StarkHash::ZERO);
    let expected_internal_l1_tx = L1HandlerTransaction {
        version: EXPECTED_VERSION,
        nonce: Nonce(StarkHash::ZERO),
        contract_address: contract_address!(l2_contract_address),
        entry_point_selector: EntryPointSelector(StarkHash::from_hex_unchecked(l2_entry_point)),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::ONE, StarkHash::from(2)].into(),
        ),
    };
    let tx = ExecutableL1HandlerTransaction {
        tx_hash: expected_internal_l1_tx
            .calculate_transaction_hash(&scraper.config.chain_id, &EXPECTED_VERSION)
            .unwrap(),
        tx: expected_internal_l1_tx,
        paid_fee_on_l1: Fee(0),
    };
    let first_expected_log = Event::L1HandlerTransaction(tx.clone());

    let expected_internal_l1_tx_2 = L1HandlerTransaction {
        nonce: Nonce(StarkHash::ONE),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::from(3), StarkHash::from(4)].into(),
        ),
        ..tx.tx
    };
    let second_expected_log = Event::L1HandlerTransaction(ExecutableL1HandlerTransaction {
        tx_hash: expected_internal_l1_tx_2
            .calculate_transaction_hash(&scraper.config.chain_id, &EXPECTED_VERSION)
            .unwrap(),
        tx: expected_internal_l1_tx_2,
        ..tx
    });

    // Assert.
    scraper.send_events_to_l1_provider().await.unwrap();
    fake_client.assert_add_events_received_with(&[first_expected_log, second_expected_log]);

    // Previous events had been scraped, should no longer appear.
    scraper.send_events_to_l1_provider().await.unwrap();
    fake_client.assert_add_events_received_with(&[]);
}

#[tokio::test]
#[ignore = "Not yet implemented: generate an l1 and an cancel event for that tx, also check an \
            abort for a different tx"]
async fn cancel_l1_handlers() {}

#[tokio::test]
#[ignore = "Not yet implemented: check that when the scraper resets all txs from the last T time
are processed"]
async fn reset() {}

#[tokio::test]
#[ignore = "Not yet implemented: check successful consume."]
async fn consume() {}
