use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_config::ValidatorId;
use apollo_node_config::node_config::NodeDynamicConfig;

use crate::config_manager::ConfigManager;

#[tokio::test]
async fn test_config_manager_update_config() {
    // Set a config manager.
    let config = ConfigManagerConfig::default();

    let consensus_dynamic_config = ConsensusDynamicConfig::default();
    let node_dynamic_config = NodeDynamicConfig {
        consensus_dynamic_config: Some(consensus_dynamic_config),
        ..Default::default()
    };
    let mut config_manager = ConfigManager::new(config, node_dynamic_config.clone());

    // Get the consensus dynamic config and assert it is the expected one.
    let consensus_dynamic_config = config_manager
        .get_consensus_dynamic_config()
        .expect("Failed to get consensus dynamic config");
    assert_eq!(
        &consensus_dynamic_config,
        node_dynamic_config.consensus_dynamic_config.as_ref().unwrap(),
        "Consensus dynamic config mismatch: {consensus_dynamic_config:#?} != {:#?}",
        node_dynamic_config.consensus_dynamic_config
    );

    // Set a new dynamic config by creating a new consensus dynamic config. For simplicity, we
    // create an arbitrary one and assert it's not the default one.
    let new_consensus_dynamic_config =
        ConsensusDynamicConfig { validator_id: ValidatorId::from(1_u8), ..Default::default() };
    assert_ne!(
        consensus_dynamic_config, new_consensus_dynamic_config,
        "Consensus dynamic config should be different: {consensus_dynamic_config:#?} != {:#?}",
        new_consensus_dynamic_config
    );
    config_manager
        .set_node_dynamic_config(NodeDynamicConfig {
            consensus_dynamic_config: Some(new_consensus_dynamic_config.clone()),
            ..Default::default()
        })
        .expect("Failed to set node dynamic config");

    // Get the post-change consensus dynamic config and assert it is the expected one.
    let consensus_dynamic_config = config_manager
        .get_consensus_dynamic_config()
        .expect("Failed to get consensus dynamic config");
    assert_eq!(
        consensus_dynamic_config, new_consensus_dynamic_config,
        "Consensus dynamic config mismatch: {consensus_dynamic_config:#?} != {:#?}",
        new_consensus_dynamic_config
    );
}
