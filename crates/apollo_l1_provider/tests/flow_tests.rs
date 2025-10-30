use std::sync::Arc;

use apollo_batcher_types::communication::MockBatcherClient;
use apollo_infra::component_client::LocalComponentClient;
use apollo_infra::component_definitions::RequestWrapper;
use apollo_infra::component_server::{LocalComponentServer, LocalServerConfig};
use apollo_l1_provider::l1_provider::L1ProviderBuilder;
use apollo_l1_provider::l1_scraper::L1Scraper;
use apollo_l1_provider::metrics::L1_PROVIDER_INFRA_METRICS;
use apollo_l1_provider::L1ProviderConfig;
use apollo_l1_provider_types::{L1ProviderRequest, L1ProviderResponse, MockL1ProviderClient};
use apollo_l1_scraper_config::config::L1ScraperConfig;
use apollo_state_sync_types::communication::MockStateSyncClient;
use papyrus_base_layer::{L1BlockReference, MockBaseLayerContract};
use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use tokio::sync::mpsc::channel;

#[tokio::test]
async fn flow_tests() {
    // Setup.
    let start_block = L1BlockReference::default();
    let historical_block = BlockHashAndNumber::default();

    // Setup the base layer.
    let mut base_layer = MockBaseLayerContract::default(); // TODO(guyn): replace this with Anvil.
    base_layer.expect_latest_l1_block().returning(move |_| Ok(Some(start_block)));
    base_layer.expect_latest_proved_block().returning(move |_| Ok(Some(historical_block)));

    // L1 provider setup.
    let mut l1_provider = L1ProviderBuilder::new(
        L1ProviderConfig::default(),
        Arc::new(MockL1ProviderClient::default()), // This isn't right
        Arc::new(MockBatcherClient::default()),    // Consider saving a copy of this to interact
        Arc::new(MockStateSyncClient::default()),  /* We'll need a copy of this if we do
                                                    * bootstrapping */
    )
    .startup_height(BlockNumber(start_block.number))
    .catchup_height(historical_block.number)
    .build();

    // This channel connects the L1Provider client to the server.
    let (tx, rx) = channel::<RequestWrapper<L1ProviderRequest, L1ProviderResponse>>(32);

    // Create the client.
    let l1_provider_client =
        LocalComponentClient::new(tx, L1_PROVIDER_INFRA_METRICS.get_local_client_metrics());

    // Create the server.
    l1_provider.initialize(vec![]).await.unwrap();
    let _l1_provider_server = LocalComponentServer::new(
        l1_provider,
        &LocalServerConfig::default(),
        rx,
        L1_PROVIDER_INFRA_METRICS.get_local_server_metrics(),
    );

    // Setup the scraper.
    let mut scraper = L1Scraper::new(
        L1ScraperConfig::default(),
        Arc::new(l1_provider_client.clone()),
        base_layer,
        &[],
        start_block,
    )
    .await
    .expect("Should be able to create the scraper");

    // Run the scraper in a separate task.
    let _scraper_task = tokio::spawn(async move {
        scraper.run().await.unwrap_or_else(|e| panic!("Error running scraper: {e:?}"));
    });
}
