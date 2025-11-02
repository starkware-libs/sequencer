use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::U256;
use apollo_base_layer_tests::anvil_base_layer::AnvilBaseLayer;
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_infra::component_client::LocalComponentClient;
use apollo_infra::component_definitions::{ComponentStarter, RequestWrapper};
use apollo_infra::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    LocalServerConfig,
};
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::l1_scraper::L1Scraper;
use apollo_l1_provider::metrics::L1_PROVIDER_INFRA_METRICS;
use apollo_l1_provider::{event_identifiers_to_track, L1ProviderConfig};
use apollo_l1_provider_types::{
    Event,
    L1ProviderClient,
    L1ProviderRequest,
    L1ProviderResponse,
    SessionState,
    ValidationStatus,
};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use papyrus_base_layer::test_utils::anvil_mine_blocks;
use papyrus_base_layer::{BaseLayerContract, L1BlockHash, L1BlockNumber, L1BlockReference};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ChainId;
use tokio::sync::mpsc::channel;

// Must wait at least 1 second, as timestamps are integer seconds.
const COOLDOWN_DURATION: Duration = Duration::from_millis(1000);
const WAIT_FOR_L1_DURATION: Duration = Duration::from_millis(10);
const NUMBER_OF_BLOCKS_TO_MINE: u64 = 100;
const CHAIN_ID: ChainId = ChainId::Mainnet;

const START_L1_BLOCK: L1BlockReference = L1BlockReference { number: 0, hash: L1BlockHash([0; 32]) };
const START_L1_BLOCK_NUMBER: L1BlockNumber = START_L1_BLOCK.number;
const START_L2_HEIGHT: BlockNumber = BlockNumber(0);
const TARGET_L2_HEIGHT: BlockNumber = BlockNumber(1);

#[tokio::test]
async fn flow_tests() {
    // Setup.
    // Setup the state sync client.
    let mut state_sync_client = MockStateSyncClient::default();
    state_sync_client.expect_get_block().returning(move |_| Ok(SyncBlock::default()));

    // Setup the base layer.
    let base_layer = AnvilBaseLayer::new(None).await;
    let contract = &base_layer.ethereum_base_layer.contract;
    anvil_mine_blocks(
        base_layer.ethereum_base_layer.config.clone(),
        NUMBER_OF_BLOCKS_TO_MINE,
        &base_layer.ethereum_base_layer.get_url().await.expect("Failed to get anvil url."),
    )
    .await;

    let finality = 0;
    let last_l1_block_number =
        base_layer.ethereum_base_layer.latest_l1_block_number(finality).await.unwrap();
    assert!(last_l1_block_number > START_L1_BLOCK_NUMBER + NUMBER_OF_BLOCKS_TO_MINE);

    // Send message from L1 to L2.
    let l2_contract_address = "0x12";
    let l2_entry_point = "0x34";
    let call_data = vec![U256::from(1_u8), U256::from(2_u8)];
    let fee = 1_u8;
    let message_to_l2 = contract
        .sendMessageToL2(
            l2_contract_address.parse().unwrap(),
            l2_entry_point.parse().unwrap(),
            call_data,
        )
        .value(U256::from(fee));
    message_to_l2.call().await.unwrap(); // Query for errors.
    let receipt = message_to_l2.send().await.unwrap().get_receipt().await.unwrap();
    let message_timestamp = base_layer
        .get_block_header(receipt.block_number.unwrap())
        .await
        .unwrap()
        .unwrap()
        .timestamp;
    assert!(message_timestamp > BlockTimestamp(0));

    // Make sure the L1 event was posted to Anvil
    let finality = 0;
    let last_l1_block_number =
        base_layer.ethereum_base_layer.latest_l1_block_number(finality).await.unwrap();
    assert!(last_l1_block_number > START_L1_BLOCK_NUMBER + NUMBER_OF_BLOCKS_TO_MINE);
    let event_filter = event_identifiers_to_track();
    let mut events = base_layer
        .ethereum_base_layer
        .events(
            // Include last block with message.
            START_L1_BLOCK_NUMBER..=last_l1_block_number + 1,
            event_filter,
        )
        .await
        .unwrap();
    assert!(events.len() == 1);
    let l1_event = events.pop().unwrap();

    // Convert the L1 event to an Apollo event, so we can get the L2 hash.
    let l1_event_converted =
        apollo_l1_provider_types::Event::from_l1_event(&CHAIN_ID, l1_event, message_timestamp.0)
            .unwrap();
    let Event::L1HandlerTransaction { l1_handler_tx, .. } = l1_event_converted else {
        panic!("L1 event converted is not a L1 handler transaction");
    };
    let l2_hash = l1_handler_tx.tx_hash;

    // Set up the L1 provider client and server.
    // This channel connects the L1Provider client to the server.
    let (tx, rx) = channel::<RequestWrapper<L1ProviderRequest, L1ProviderResponse>>(32);

    // Create the provider client.
    let l1_provider_client =
        LocalComponentClient::new(tx, L1_PROVIDER_INFRA_METRICS.get_local_client_metrics());

    // L1 provider setup.
    let l1_provider_config = L1ProviderConfig {
        new_l1_handler_cooldown_seconds: COOLDOWN_DURATION,
        ..Default::default()
    };
    let l1_provider = L1ProviderBuilder::new(
        l1_provider_config,
        Arc::new(l1_provider_client.clone()),
        Arc::new(MockBatcherClient::default()), // Consider saving a copy of this to interact
        Arc::new(state_sync_client),
    )
    .startup_height(START_L2_HEIGHT)
    .catchup_height(TARGET_L2_HEIGHT)
    .build();

    // Create the server.
    let mut l1_provider_server = LocalComponentServer::new(
        l1_provider,
        &LocalServerConfig::default(),
        rx,
        L1_PROVIDER_INFRA_METRICS.get_local_server_metrics(),
    );
    // Start the server:
    tokio::spawn(async move {
        l1_provider_server.start().await;
    });

    // Set up the L1 scraper and run it as a server.
    let l1_scraper_config = L1ScraperConfig {
        polling_interval_seconds: COOLDOWN_DURATION,
        chain_id: CHAIN_ID,
        ..Default::default()
    };
    let mut scraper = L1Scraper::new(
        l1_scraper_config,
        Arc::new(l1_provider_client.clone()),
        base_layer,
        &[],
        START_L1_BLOCK,
    )
    .await
    .expect("Should be able to create the scraper");

    // Run the scraper in a separate task.
    tokio::spawn(async move {
        scraper.start().await;
    });
    tokio::time::sleep(WAIT_FOR_L1_DURATION).await;

    // Test.
    let next_block_height = BlockNumber(TARGET_L2_HEIGHT.0 + 1);

    // Check that we can validate this message even though no time has passed.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );

    // Test that we do not propose anything before the cooldown is over.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let n_txs = 1;
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(txs.is_empty());

    // Sleep at least one second more than the  cooldown to make sure we are not failing due to
    // fractional seconds.
    tokio::time::sleep(COOLDOWN_DURATION + Duration::from_secs(1)).await;

    // Test that we propose after the cooldown is over.
    l1_provider_client.start_block(SessionState::Propose, next_block_height).await.unwrap();
    let txs = l1_provider_client.get_txs(n_txs, next_block_height).await.unwrap();
    assert!(!txs.is_empty());

    // Check that we can validate this message after the cooldown, too.
    l1_provider_client.start_block(SessionState::Validate, next_block_height).await.unwrap();
    assert_eq!(
        l1_provider_client.validate(l2_hash, next_block_height).await.unwrap(),
        ValidationStatus::Validated
    );
}
