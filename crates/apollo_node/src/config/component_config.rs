// Re-export component config types from the new crate
pub use apollo_node_config::{
    ComponentConfig,
    ActiveComponentExecutionConfig,
    ActiveComponentExecutionMode,
    ExpectedComponentConfig,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
    MAX_CONCURRENCY,
};

#[cfg(any(feature = "testing", test))]
pub use apollo_node_config::set_urls_to_localhost;