use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Uint, U256};
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_infra::component_client::LocalComponentClient;
use apollo_infra::component_definitions::{ComponentStarter, RequestWrapper};
use apollo_infra::component_server::{
    ComponentServerStarter,
    LocalComponentServer,
    LocalServerConfig,
};
use apollo_integration_tests::anvil_base_layer::AnvilBaseLayer;
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::l1_scraper::L1Scraper;
use apollo_l1_provider::metrics::L1_PROVIDER_INFRA_METRICS;
use apollo_l1_provider::{event_identifiers_to_track, L1ProviderConfig};
use apollo_l1_provider_types::{Event, L1ProviderRequest, L1ProviderResponse};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use papyrus_base_layer::ethereum_base_layer_contract::Starknet::LogMessageToL2;
use papyrus_base_layer::test_utils::anvil_mine_blocks;
use papyrus_base_layer::{BaseLayerContract, L1BlockHash, L1BlockNumber, L1BlockReference};
use starknet_api::block::{BlockNumber, BlockTimestamp};
use starknet_api::core::ChainId;
use starknet_api::transaction::TransactionHash;
use tokio::sync::mpsc::channel;

// Must wait at least 1 second, as timestamps are integer seconds.
pub(crate) const COOLDOWN_DURATION: Duration = Duration::from_millis(1000);
pub(crate) const WAIT_FOR_L1_DURATION: Duration = Duration::from_millis(10);
const NUMBER_OF_BLOCKS_TO_MINE: u64 = 100;
const CHAIN_ID: ChainId = ChainId::Mainnet;
const L1_CONTRACT_ADDRESS: &str = "0x12";
const L2_ENTRY_POINT: &str = "0x34";
const CALL_DATA: &[u8] = &[1_u8, 2_u8];

const START_L1_BLOCK: L1BlockReference = L1BlockReference { number: 0, hash: L1BlockHash([0; 32]) };
const START_L1_BLOCK_NUMBER: L1BlockNumber = START_L1_BLOCK.number;
const START_L2_HEIGHT: BlockNumber = BlockNumber(0);
pub(crate) const TARGET_L2_HEIGHT: BlockNumber = BlockNumber(1);

/// Setup an anvil base layer with some blocks on it.
pub(crate) async fn setup_anvil_base_layer() -> AnvilBaseLayer {
    let base_layer = AnvilBaseLayer::new(None).await;
    anvil_mine_blocks(
        base_layer.ethereum_base_layer.config.clone(),
        NUMBER_OF_BLOCKS_TO_MINE,
        &base_layer.ethereum_base_layer.get_url().await.expect("Failed to get anvil url."),
    )
    .await;
    base_layer
}

/// Set up the scraper and provider, return a provider client.
pub(crate) async fn setup_scraper_and_provider(
    base_layer: &AnvilBaseLayer,
) -> LocalComponentClient<L1ProviderRequest, L1ProviderResponse> {
    // Setup the state sync client.
    let mut state_sync_client = MockStateSyncClient::default();
    state_sync_client.expect_get_block().returning(move |_| Ok(SyncBlock::default()));

    // Set up the L1 provider client and server.
    // This channel connects the L1Provider client to the server.
    let (tx, rx) = channel::<RequestWrapper<L1ProviderRequest, L1ProviderResponse>>(32);

    // Create the provider client.
    let l1_provider_client =
        LocalComponentClient::new(tx, L1_PROVIDER_INFRA_METRICS.get_local_client_metrics());

    // L1 provider setup.
    let l1_provider_config = L1ProviderConfig {
        new_l1_handler_cooldown_seconds: COOLDOWN_DURATION,
        // Use the same time as new l1 handler cooldown, for simplicity.
        l1_handler_cancellation_timelock_seconds: COOLDOWN_DURATION,
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
        base_layer.ethereum_base_layer.clone(),
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

    l1_provider_client
}

/// Send a message from L1 to L2 and return the transaction hash on L2.
pub(crate) async fn send_message_from_l1_to_l2(
    base_layer: &AnvilBaseLayer,
) -> (TransactionHash, Uint<256, 4>) {
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
    let call_data = CALL_DATA.iter().map(|x| U256::from(*x)).collect();
    let fee = 1_u8;
    let message_to_l2 = contract
        .sendMessageToL2(
            L1_CONTRACT_ADDRESS.parse().unwrap(),
            L2_ENTRY_POINT.parse().unwrap(),
            call_data,
        )
        .value(U256::from(fee));
    let receipt = message_to_l2.send().await.unwrap().get_receipt().await.unwrap();
    let message_to_l2_event = receipt
        .decoded_log::<LogMessageToL2>()
        .expect("Failed to decode LogMessageToL2 event from transaction receipt");
    let nonce = message_to_l2_event.nonce;
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
    (l1_handler_tx.tx_hash, nonce)
}

// Need to allow dead code as this is only used in some of the test crates.
#[allow(dead_code)]
pub(crate) async fn send_cancellation_request(base_layer: &AnvilBaseLayer, nonce: Uint<256, 4>) {
    let contract = &base_layer.ethereum_base_layer.contract;
    let call_data = CALL_DATA.iter().map(|x| U256::from(*x)).collect();
    let cancellation_request = contract.startL1ToL2MessageCancellation(
        L1_CONTRACT_ADDRESS.parse().unwrap(),
        L2_ENTRY_POINT.parse().unwrap(),
        call_data,
        nonce,
    );
    let _receipt = cancellation_request.send().await.unwrap().get_receipt().await.unwrap();
}
