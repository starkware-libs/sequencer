use std::sync::Arc;

use async_trait::async_trait;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::ConsensusError;
use papyrus_consensus_orchestrator::cende::CendeAmbassador;
use papyrus_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_protobuf::consensus::{ConsensusMessage, ProposalPart, StreamMessage};
use starknet_api::block::BlockNumber;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_infra_utils::type_name::short_type_name;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use tracing::{error, info};

use crate::config::ConsensusManagerConfig;

// TODO(Dan, Guy): move to config.
pub const BROADCAST_BUFFER_SIZE: usize = 100;
pub const CONSENSUS_PROPOSALS_TOPIC: &str = "consensus_proposals";
pub const CONSENSUS_VOTES_TOPIC: &str = "consensus_votes";

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
    pub state_sync_client: SharedStateSyncClient,
}

impl ConsensusManager {
    pub fn new(
        config: ConsensusManagerConfig,
        batcher_client: SharedBatcherClient,
        state_sync_client: SharedStateSyncClient,
    ) -> Self {
        Self { config, batcher_client, state_sync_client }
    }

    pub async fn run(&self) -> Result<(), ConsensusError> {
        let mut network_manager =
            NetworkManager::new(self.config.consensus_config.network_config.clone(), None);

        let proposals_broadcast_channels = network_manager
            .register_broadcast_topic::<StreamMessage<ProposalPart>>(
                Topic::new(CONSENSUS_PROPOSALS_TOPIC),
                BROADCAST_BUFFER_SIZE,
            )
            .expect("Failed to register broadcast topic");

        let votes_broadcast_channels = network_manager
            .register_broadcast_topic::<ConsensusMessage>(
                Topic::new(CONSENSUS_VOTES_TOPIC),
                BROADCAST_BUFFER_SIZE,
            )
            .expect("Failed to register broadcast topic");

        let BroadcastTopicChannels {
            broadcasted_messages_receiver: inbound_network_receiver,
            broadcast_topic_client: outbound_network_sender,
        } = proposals_broadcast_channels;

        let (outbound_internal_sender, inbound_internal_receiver, mut stream_handler_task_handle) =
            StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

        let observer_height =
            self.batcher_client.get_height().await.map(|h| h.height).map_err(|e| {
                error!("Failed to get height from batcher: {:?}", e);
                ConsensusError::Other("Failed to get height from batcher".to_string())
            })?;
        let active_height = if self.config.consensus_config.start_height == observer_height {
            // Setting `start_height` is only used to enable consensus starting immediately without
            // observing the first height. This means consensus may return to a height
            // it has already voted on, risking equivocation. This is only safe to do if we
            // restart all nodes at this height.
            observer_height
        } else {
            BlockNumber(observer_height.0 + 1)
        };

        let context = SequencerConsensusContext::new(
            Arc::clone(&self.batcher_client),
            outbound_internal_sender,
            votes_broadcast_channels.broadcast_topic_client.clone(),
            self.config.consensus_config.num_validators,
            self.config.consensus_config.chain_id.clone(),
            Arc::new(CendeAmbassador::new()),
        );

        let mut network_handle = tokio::task::spawn(network_manager.run());
        let consensus_task = papyrus_consensus::run_consensus(
            context,
            active_height,
            observer_height,
            self.config.consensus_config.validator_id,
            self.config.consensus_config.consensus_delay,
            self.config.consensus_config.timeouts.clone(),
            votes_broadcast_channels.into(),
            inbound_internal_receiver,
            futures::stream::pending(),
        );

        tokio::select! {
            consensus_result = consensus_task => {
                match consensus_result {
                    Ok(_) => panic!("Consensus task finished unexpectedly"),
                    Err(e) => Err(e),
                }
            },
            network_result = &mut network_handle => {
                panic!("Consensus' network task finished unexpectedly: {:?}", network_result);
            }
            stream_handler_result = &mut stream_handler_task_handle => {
                panic!("Consensus' stream handler task finished unexpectedly: {:?}", stream_handler_result);
            }
        }
    }
}

pub fn create_consensus_manager(
    config: ConsensusManagerConfig,
    batcher_client: SharedBatcherClient,
    state_sync_client: SharedStateSyncClient,
) -> ConsensusManager {
    ConsensusManager::new(config, batcher_client, state_sync_client)
}

#[async_trait]
impl ComponentStarter for ConsensusManager {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.map_err(|e| {
            error!("Error running component ConsensusManager: {:?}", e);
            ComponentError::InternalComponentError
        })
    }
}
