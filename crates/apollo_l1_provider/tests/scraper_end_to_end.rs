use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::U256;
use apollo_l1_provider::event_identifiers_to_track;
use apollo_l1_provider::l1_scraper::{fetch_start_block, L1Scraper, L1ScraperConfig};
use apollo_l1_provider_types::{Event, MockL1ProviderClient};
use mockall::predicate::eq;
use mockall::Sequence;
use papyrus_base_layer::ethereum_base_layer_contract::{EthereumBaseLayerContract, Starknet};
use papyrus_base_layer::test_utils::{
    anvil_instance_from_config,
    ethereum_base_layer_config_for_anvil,
    DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS,
};
use papyrus_base_layer::BaseLayerContract;
use starknet_api::block::BlockTimestamp;
use starknet_api::contract_address;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{L1HandlerTransaction, TransactionHasher, TransactionVersion};

pub fn in_ci() -> bool {
    std::env::var("CI").is_ok()
}

#[tokio::test]
async fn scraper_end_to_end() {
    if !in_ci() {
        return;
    }

    // Setup.
    let base_layer_config = ethereum_base_layer_config_for_anvil(None);
    let _anvil_server_guard = anvil_instance_from_config(&base_layer_config);
    let mut l1_provider_client = MockL1ProviderClient::default();
    let base_layer = EthereumBaseLayerContract::new(base_layer_config);

    // Deploy a fresh Starknet contract on Anvil from the bytecode in the JSON file.
    Starknet::deploy(base_layer.contract.provider().clone()).await.unwrap();

    // Send messages from L1 to L2.
    let l2_contract_address = "0x12";
    let l2_entry_point = "0x34";
    let message_to_l2_0 = base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(1_u8), U256::from(2_u8)],
    );
    let message_to_l2_1 = base_layer.contract.sendMessageToL2(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(3_u8), U256::from(4_u8)],
    );
    let nonce_of_message_to_l2_0 = U256::from(0_u8);
    let request_cancel_message_0 = base_layer.contract.startL1ToL2MessageCancellation(
        l2_contract_address.parse().unwrap(),
        l2_entry_point.parse().unwrap(),
        vec![U256::from(1_u8), U256::from(2_u8)],
        nonce_of_message_to_l2_0,
    );

    // Send the transactions to Anvil, and record the timestamps of the blocks they are included in.
    let mut l1_handler_timestamps: Vec<BlockTimestamp> = Vec::with_capacity(2);
    for msg in &[message_to_l2_0, message_to_l2_1] {
        let receipt = msg.send().await.unwrap().get_receipt().await.unwrap();
        l1_handler_timestamps.push(
            base_layer
                .get_block_header(receipt.block_number.unwrap())
                .await
                .unwrap()
                .unwrap()
                .timestamp,
        );
    }

    let cancel_receipt =
        request_cancel_message_0.send().await.unwrap().get_receipt().await.unwrap();
    let cancel_timestamp = base_layer
        .get_block_header(cancel_receipt.block_number.unwrap())
        .await
        .unwrap()
        .unwrap()
        .timestamp;

    const EXPECTED_VERSION: TransactionVersion = TransactionVersion(StarkHash::ZERO);
    let expected_l1_handler_0 = L1HandlerTransaction {
        version: EXPECTED_VERSION,
        nonce: Nonce(StarkHash::ZERO),
        contract_address: contract_address!(l2_contract_address),
        entry_point_selector: EntryPointSelector(StarkHash::from_hex_unchecked(l2_entry_point)),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::ONE, StarkHash::from(2)].into(),
        ),
    };
    let default_chain_id = L1ScraperConfig::default().chain_id;
    let tx_hash_first_tx = expected_l1_handler_0
        .calculate_transaction_hash(&default_chain_id, &EXPECTED_VERSION)
        .unwrap();
    let expected_executable_l1_handler_0 = ExecutableL1HandlerTransaction {
        tx_hash: tx_hash_first_tx,
        tx: expected_l1_handler_0,
        paid_fee_on_l1: Fee(0),
    };
    let first_expected_log = Event::L1HandlerTransaction {
        l1_handler_tx: expected_executable_l1_handler_0.clone(),
        block_timestamp: l1_handler_timestamps[0],
        scrape_timestamp: l1_handler_timestamps[0].0,
    };

    let expected_l1_handler_1 = L1HandlerTransaction {
        nonce: Nonce(StarkHash::ONE),
        calldata: Calldata(
            vec![DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS, StarkHash::from(3), StarkHash::from(4)].into(),
        ),
        ..expected_executable_l1_handler_0.tx
    };
    let expected_executable_l1_handler_1 = ExecutableL1HandlerTransaction {
        tx_hash: expected_l1_handler_1
            .calculate_transaction_hash(&default_chain_id, &EXPECTED_VERSION)
            .unwrap(),
        tx: expected_l1_handler_1,
        ..expected_executable_l1_handler_0
    };
    let second_expected_log = Event::L1HandlerTransaction {
        l1_handler_tx: expected_executable_l1_handler_1,
        block_timestamp: l1_handler_timestamps[1],
        scrape_timestamp: l1_handler_timestamps[1].0,
    };

    let expected_cancel_message = Event::TransactionCancellationStarted {
        tx_hash: tx_hash_first_tx,
        cancellation_request_timestamp: cancel_timestamp,
    };

    let mut sequence = Sequence::new();
    // Expect first call to return all the events defined further down.
    l1_provider_client
        .expect_add_events()
        .once()
        .in_sequence(&mut sequence)
        .with(eq(vec![first_expected_log, second_expected_log, expected_cancel_message]))
        .returning(|_| Ok(()));

    // Expect second call to return nothing, no events left to scrape.
    l1_provider_client.expect_add_events().once().in_sequence(&mut sequence).returning(|_| Ok(()));

    let l1_scraper_config = L1ScraperConfig {
        // Start scraping far enough back to capture all of the events created before.
        startup_rewind_time_seconds: Duration::from_secs(100),
        ..Default::default()
    };
    let l1_start_block = fetch_start_block(&base_layer, &l1_scraper_config).await.unwrap();
    let mut scraper = L1Scraper::new(
        l1_scraper_config,
        Arc::new(l1_provider_client),
        base_layer.clone(),
        event_identifiers_to_track(),
        l1_start_block,
    )
    .await
    .unwrap();

    // Test.
    scraper.send_events_to_l1_provider().await.unwrap();

    // Previous events had been scraped, should no longer appear.
    scraper.send_events_to_l1_provider().await.unwrap();
}
