use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::NodeDescriptor;
use apollo_node_config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use apollo_node_config::node_config::SequencerNodeConfig;
use apollo_state_sync_config::config::CentralSyncClientConfig;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("sync").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(20);
    // Node layout (index → type): 0 = consolidated, 1 = consolidated.
    // Node 1 is configured as the central sync node.
    const CENTRAL_SYNC_NODE: usize = 1;

    let node_descriptors = vec![NodeDescriptor::consolidated(), NodeDescriptor::consolidated()];

    let mut integration_test_manager = IntegrationTestManager::new(
        node_descriptors,
        None,
        TestIdentifier::SyncFlowIntegrationTest,
    )
    .await;

    let update_config_disable_everything_but_sync = |config: &mut SequencerNodeConfig| {
        config.components.batcher.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.gateway.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.mempool.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.mempool_p2p.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.l1_events_provider.execution_mode =
            ReactiveComponentExecutionMode::Disabled;
        config.components.consensus_manager.execution_mode = ActiveComponentExecutionMode::Disabled;
        config.components.http_server.execution_mode = ActiveComponentExecutionMode::Disabled;
        config.components.l1_events_scraper.execution_mode = ActiveComponentExecutionMode::Disabled;
    };

    let update_config_use_central_sync = |config: &mut SequencerNodeConfig| {
        config.state_sync_config.as_mut().unwrap().static_config.central_sync_client_config =
            Some(CentralSyncClientConfig::default());
        config.state_sync_config.as_mut().unwrap().static_config.p2p_sync_client_config = None;
    };

    let node_indices = integration_test_manager.get_node_indices();
    integration_test_manager
        .modify_config_idle_nodes(node_indices.clone(), update_config_disable_everything_but_sync);
    integration_test_manager
        .modify_config_idle_nodes([CENTRAL_SYNC_NODE].into(), update_config_use_central_sync);

    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.await_sync_block_on_all_running_nodes(BLOCK_TO_WAIT_FOR).await;

    info!("Sync flow integration test completed successfully!");
}
