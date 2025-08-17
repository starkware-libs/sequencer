use apollo_block_hash_calculator_types::communication::{
    FinalizeBlockHashInput,
    InitializeBlockHashInput,
};
use apollo_block_hash_calculator_types::errors::BlockHashCalculatorResult;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_api::block::BlockHash;
use starknet_types_core::felt::Felt;
use tracing::info;

/// The Apollo BlockHashCalculator component responsible for calculating block hashes.
pub struct BlockHashCalculator {}

impl BlockHashCalculator {
    pub fn initialize_block_hash(
        &self,
        _input: InitializeBlockHashInput,
    ) -> BlockHashCalculatorResult<Felt> {
        todo!("Implement block hash calculation logic")
    }

    pub fn finalize_block_hash(
        &self,
        _input: FinalizeBlockHashInput,
    ) -> BlockHashCalculatorResult<BlockHash> {
        todo!("Implement block hash finalization logic")
    }
}

#[async_trait]
impl ComponentStarter for BlockHashCalculator {
    async fn start(&mut self) {
        info!("Starting BlockHashCalculator component");
        default_component_start_fn::<Self>().await;
    }
}
