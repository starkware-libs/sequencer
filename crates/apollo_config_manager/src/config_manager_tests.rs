use apollo_batcher_config::config::BatcherDynamicConfig;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_config::ValidatorId;
use apollo_consensus_orchestrator_config::config::{
    parse_price_per_height,
    ContextDynamicConfig,
    PricePerHeight,
};
use apollo_node_config::node_config::NodeDynamicConfig;
use validator::Validate;

use crate::config_manager::ConfigManager;

#[tokio::test]
async fn config_manager_update_config() {
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

#[tokio::test]
async fn config_manager_get_batcher_dynamic_config() {
    let config = ConfigManagerConfig::default();
    let batcher_dynamic_config = BatcherDynamicConfig::default();
    let node_dynamic_config = NodeDynamicConfig {
        batcher_dynamic_config: Some(batcher_dynamic_config.clone()),
        ..Default::default()
    };
    let config_manager = ConfigManager::new(config, node_dynamic_config);

    let retrieved =
        config_manager.get_batcher_dynamic_config().expect("Failed to get batcher dynamic config");
    assert_eq!(
        retrieved, batcher_dynamic_config,
        "Batcher dynamic config mismatch: {retrieved:#?} != {batcher_dynamic_config:#?}",
    );
}

#[test]
fn test_context_dynamic_config_serialize_deserialize() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 100, price: 10_000_000_000 },
            PricePerHeight { height: 500, price: 20_000_000_000 },
            PricePerHeight { height: 1000, price: 30_000_000_000 },
        ],
    };

    // Serialize to JSON
    let json = serde_json::to_string(&config).expect("Failed to serialize");

    // Deserialize back
    let deserialized: ContextDynamicConfig =
        serde_json::from_str(&json).expect("Failed to deserialize");

    // Should match original
    assert_eq!(deserialized, config);
}

#[test]
fn test_context_dynamic_config_serialize_deserialize_empty() {
    let config = ContextDynamicConfig { min_l2_gas_price_per_height: vec![] };

    let json = serde_json::to_string(&config).expect("Failed to serialize");
    let deserialized: ContextDynamicConfig =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized, config);
}

#[test]
fn test_parse_price_per_height_with_whitespace() {
    // Test that whitespace is properly trimmed during parsing
    let data = " 100 : 10000000000 , 500 :  20000000000 ";
    // This func is used for deserialization of the min_l2_gas_price_per_height field.
    let result = parse_price_per_height(data).expect("Failed to parse");

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].height, 100);
    assert_eq!(result[0].price, 10_000_000_000);
    assert_eq!(result[1].height, 500);
    assert_eq!(result[1].price, 20_000_000_000);
}

#[test]
fn test_context_dynamic_config_validation_valid() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 100, price: 10_000_000_000 },
            PricePerHeight { height: 500, price: 20_000_000_000 },
            PricePerHeight { height: 1000, price: 30_000_000_000 },
        ],
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_context_dynamic_config_validation_price_below_minimum() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 100, price: 500_000_000 }, // Below 8 gwei
        ],
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_context_dynamic_config_validation_heights_not_in_order() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 500, price: 10_000_000_000 },
            PricePerHeight { height: 100, price: 20_000_000_000 }, // Out of order
        ],
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_context_dynamic_config_validation_duplicate_heights() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 100, price: 10_000_000_000 },
            PricePerHeight { height: 100, price: 20_000_000_000 }, // Duplicate
        ],
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_context_dynamic_config_validation_price_at_minimum() {
    let config = ContextDynamicConfig {
        min_l2_gas_price_per_height: vec![
            PricePerHeight { height: 100, price: 8_000_000_000 }, // Exactly 8 gwei
        ],
    };

    assert!(config.validate().is_ok());
}
