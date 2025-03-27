use starknet_sequencer_infra::component_server::WrapperServer;

use crate::consensus_manager::ConsensusManager;

pub type ConsensusManagerServer = WrapperServer<ConsensusManager>;
