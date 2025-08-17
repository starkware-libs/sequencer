use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use tracing::info;

/// The Apollo BlockHashCalculator component responsible for calculating block hashes.
pub struct BlockHashCalculator {
    // Empty for now - fields will be added as needed
}

impl BlockHashCalculator {
    pub fn new() -> Self {
        Self {
            // Empty for now
        }
    }
}

impl Default for BlockHashCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ComponentStarter for BlockHashCalculator {
    async fn start(&mut self) {
        info!("Starting BlockHashCalculator component");
        default_component_start_fn::<Self>().await;
    }
}
