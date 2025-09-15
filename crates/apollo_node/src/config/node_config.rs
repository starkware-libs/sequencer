// Re-export the main config types from the new crate
pub use apollo_node_config::{
    node_command,
    ComponentConfig,
    ConfigExpectation,
    ConfigPointersMap,
    ConfigPresence,
    DeploymentBaseAppConfig,
    MonitoringConfig,
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    CONFIG_SCHEMA_PATH,
    CONFIG_SECRETS_SCHEMA_PATH,
    POINTER_TARGET_VALUE,
};