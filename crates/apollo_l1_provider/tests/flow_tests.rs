use std::sync::Arc;

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
use apollo_l1_provider_types::{L1ProviderRequest, L1ProviderResponse};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use papyrus_base_layer::test_utils::anvil_mine_blocks;
use papyrus_base_layer::{BaseLayerContract, L1BlockNumber, L1BlockReference};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ChainId;
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn flow_tests() {
    // Setup.
    const NUMBER_OF_BLOCKS_TO_MINE: u64 = 100;
    let chain_id = ChainId::Mainnet;
    let start_l1_block = L1BlockReference::default();
    let start_l1_block_number: L1BlockNumber = start_l1_block.number;
    let start_l2_height = BlockNumber(0);
    let target_l2_height = BlockNumber(1);

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
    assert!(last_l1_block_number > start_l1_block_number + NUMBER_OF_BLOCKS_TO_MINE);
    let event_filter = event_identifiers_to_track();
    let events = base_layer
        .ethereum_base_layer
        .events(
            // Include last block with message.
            start_l1_block_number..=last_l1_block_number + 1,
            event_filter,
        )
        .await
        .unwrap();
    assert!(events.len() == 1);

    // Set up the L1 provider client and server.
    // This channel connects the L1Provider client to the server.
    let (tx, rx) = channel::<RequestWrapper<L1ProviderRequest, L1ProviderResponse>>(32);

    // Create the provider client.
    let l1_provider_client =
        LocalComponentClient::new(tx, L1_PROVIDER_INFRA_METRICS.get_local_client_metrics());

    // L1 provider setup.
    let l1_provider = L1ProviderBuilder::new(
        L1ProviderConfig::default(),
        Arc::new(l1_provider_client.clone()),
        Arc::new(MockBatcherClient::default()), // Consider saving a copy of this to interact
        Arc::new(state_sync_client),
    )
    .startup_height(start_l2_height)
    .catchup_height(target_l2_height)
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
    let l1_scraper_config = L1ScraperConfig { chain_id, ..Default::default() };
    let mut scraper = L1Scraper::new(
        l1_scraper_config,
        Arc::new(l1_provider_client.clone()),
        base_layer,
        &[],
        start_l1_block,
    )
    .await
    .expect("Should be able to create the scraper");

    // Run the scraper in a separate task.
    tokio::spawn(async move {
        scraper.start().await;
    });
    // TODO(guyn): add the actual test here.
}
