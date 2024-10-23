use starknet_sequencer_infra::component_server::WrapperServer;

use crate::consensus_manager::ConsensusManager;

pub type ConsensusManagerServer = WrapperServer<ConsensusManager>;

pub fn create_consensus_manager_server(
    consensus_manager: ConsensusManager,
) -> ConsensusManagerServer {
    WrapperServer::new(consensus_manager)
}
