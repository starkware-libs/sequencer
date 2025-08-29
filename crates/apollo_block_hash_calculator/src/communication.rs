use apollo_block_hash_calculator_types::communication::{BlockHashCalculatorRequest, BlockHashCalculatorResponse};
use apollo_infra::component_definitions::ComponentRequestHandler;
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use async_trait::async_trait;

use crate::block_hash_calculator::BlockHashCalculator;

pub type LocalBlockHashCalculatorServer =
    LocalComponentServer<BlockHashCalculator, BlockHashCalculatorRequest, BlockHashCalculatorResponse>;
pub type RemoteBlockHashCalculatorServer =
    RemoteComponentServer<BlockHashCalculatorRequest, BlockHashCalculatorResponse>;

#[async_trait]
impl ComponentRequestHandler<BlockHashCalculatorRequest, BlockHashCalculatorResponse> for BlockHashCalculator {
    async fn handle_request(&mut self, _request: BlockHashCalculatorRequest) -> BlockHashCalculatorResponse {
        // Empty for now - will handle requests when they are defined
        unreachable!("No requests defined yet")
    }
}
