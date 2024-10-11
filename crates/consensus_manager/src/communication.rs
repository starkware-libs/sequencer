use async_trait::async_trait;
use starknet_consensus_manager_types::communication::{
    ConsensusManagerRequest,
    ConsensusManagerResponse,
};
use starknet_mempool_infra::component_definitions::ComponentRequestHandler;
use starknet_mempool_infra::component_server::WrapperServer;

use crate::consensus_manager::ConsensusManager;

pub type ConsensusManagerServer = WrapperServer<ConsensusManager>;

pub fn create_consensus_manager_server(
    consensus_manager: ConsensusManager,
) -> ConsensusManagerServer {
    WrapperServer::new(consensus_manager)
}

#[async_trait]
impl ComponentRequestHandler<ConsensusManagerRequest, ConsensusManagerResponse>
    for ConsensusManager
{
    async fn handle_request(
        &mut self,
        request: ConsensusManagerRequest,
    ) -> ConsensusManagerResponse {
        match request {
            ConsensusManagerRequest::ConsensusManagerFnOne(_consensus_manager_input) => {
                // TODO(Tsabary/Matan): Invoke a function that returns a
                // ConsensusManagerResult<ConsensusManagerFnOneReturnValue>, and return
                // the ConsensusManagerResponse::ConsensusManagerFnOneInput accordingly.
                unimplemented!()
            }
            ConsensusManagerRequest::ConsensusManagerFnTwo(_consensus_manager_input) => {
                // TODO(Tsabary/Matan): Invoke a function that returns a
                // ConsensusManagerResult<ConsensusManagerFnTwoReturnValue>, and return
                // the ConsensusManagerResponse::ConsensusManagerFnTwoInput accordingly.
                unimplemented!()
            }
        }
    }
}
