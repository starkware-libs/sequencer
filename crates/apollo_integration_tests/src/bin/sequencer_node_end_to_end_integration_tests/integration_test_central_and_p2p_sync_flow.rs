use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use apollo_node::config::node_config::SequencerNodeConfig;
use apollo_state_sync::config::CentralSyncClientConfig;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("sync").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(20);
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 2;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;

    const CENTRAL_SYNC_NODE: usize = 1;

    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        TestIdentifier::SyncFlowIntegrationTest,
    )
    .await;

    let update_config_disable_everything_but_sync = |config: &mut SequencerNodeConfig| {
        config.components.batcher.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.gateway.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.mempool.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.mempool_p2p.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.l1_provider.execution_mode = ReactiveComponentExecutionMode::Disabled;
        config.components.consensus_manager.execution_mode = ActiveComponentExecutionMode::Disabled;
        config.components.http_server.execution_mode = ActiveComponentExecutionMode::Disabled;
        config.components.l1_scraper.execution_mode = ActiveComponentExecutionMode::Disabled;
    };

    let update_config_use_central_sync = |config: &mut SequencerNodeConfig| {
        config.state_sync_config.as_mut().unwrap().central_sync_client_config =
            Some(CentralSyncClientConfig::default());
        config.state_sync_config.as_mut().unwrap().p2p_sync_client_config = None;
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
