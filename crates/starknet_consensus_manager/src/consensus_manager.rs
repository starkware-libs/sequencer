#[cfg(test)]
#[path = "consensus_manager_test.rs"]
mod consensus_manager_test;

use std::sync::Arc;

use apollo_reverts::revert_blocks_and_eternal_pending;
use async_trait::async_trait;
use futures::channel::mpsc;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::metrics::{BroadcastNetworkMetrics, NetworkMetrics};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage, Vote};
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::RevertBlockInput;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_consensus::stream_handler::{StreamHandler, CHANNEL_BUFFER_LENGTH};
use starknet_consensus::types::ConsensusError;
use starknet_consensus_orchestrator::cende::CendeAmbassador;
use starknet_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use starknet_infra_utils::type_name::short_type_name;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use tracing::info;

use crate::config::ConsensusManagerConfig;
use crate::metrics::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_NUM_SENT_MESSAGES,
};

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
    pub state_sync_client: SharedStateSyncClient,
    pub class_manager_client: SharedClassManagerClient,
}

impl ConsensusManager {
    pub fn new(
        config: ConsensusManagerConfig,
        batcher_client: SharedBatcherClient,
        state_sync_client: SharedStateSyncClient,
        class_manager_client: SharedClassManagerClient,
    ) -> Self {
        Self { config, batcher_client, state_sync_client, class_manager_client }
    }

    pub async fn run(&self) -> Result<(), ConsensusError> {
        if self.config.revert_config.should_revert {
            self.revert_batcher_blocks(self.config.revert_config.revert_up_to_and_including).await;
        }

        let network_manager_metrics = Some(NetworkMetrics {
            num_connected_peers: CONSENSUS_NUM_CONNECTED_PEERS,
            broadcast_metrics: Some(BroadcastNetworkMetrics {
                num_sent_broadcast_messages: CONSENSUS_NUM_SENT_MESSAGES,
                num_received_broadcast_messages: CONSENSUS_NUM_RECEIVED_MESSAGES,
            }),
            sqmr_metrics: None,
        });
        let mut network_manager =
            NetworkManager::new(self.config.network_config.clone(), None, network_manager_metrics);

        let proposals_broadcast_channels = network_manager
            .register_broadcast_topic::<StreamMessage<ProposalPart, HeightAndRound>>(
                Topic::new(self.config.proposals_topic.clone()),
                self.config.broadcast_buffer_size,
            )
            .expect("Failed to register broadcast topic");

        let votes_broadcast_channels = network_manager
            .register_broadcast_topic::<Vote>(
                Topic::new(self.config.votes_topic.clone()),
                self.config.broadcast_buffer_size,
            )
            .expect("Failed to register broadcast topic");

        let BroadcastTopicChannels {
            broadcasted_messages_receiver: inbound_network_receiver,
            broadcast_topic_client: outbound_network_sender,
        } = proposals_broadcast_channels;

        let (inbound_internal_sender, inbound_internal_receiver) =
            mpsc::channel(CHANNEL_BUFFER_LENGTH);
        let (outbound_internal_sender, outbound_internal_receiver) =
            mpsc::channel(CHANNEL_BUFFER_LENGTH);
        let stream_handler = StreamHandler::new(
            inbound_internal_sender,
            inbound_network_receiver,
            outbound_internal_receiver,
            outbound_network_sender,
        );

        let observer_height = self
            .batcher_client
            .get_height()
            .await
            .expect("Failed to get observer_height from batcher")
            .height;
        let active_height = if self.config.immediate_active_height == observer_height {
            // Setting `start_height` is only used to enable consensus starting immediately without
            // observing the first height. This means consensus may return to a height
            // it has already voted on, risking equivocation. This is only safe to do if we
            // restart all nodes at this height.
            observer_height
        } else {
            BlockNumber(observer_height.0 + 1)
        };

        let context = SequencerConsensusContext::new(
            self.config.context_config.clone(),
            Arc::clone(&self.class_manager_client),
            Arc::clone(&self.state_sync_client),
            Arc::clone(&self.batcher_client),
            outbound_internal_sender,
            votes_broadcast_channels.broadcast_topic_client.clone(),
            Arc::new(CendeAmbassador::new(
                self.config.cende_config.clone(),
                Arc::clone(&self.class_manager_client),
            )),
            // TODO(Asmaa): Send EthToStrkOracleClient to the context.
            None,
        );

        let network_task = tokio::spawn(network_manager.run());
        let stream_handler_task = tokio::spawn(stream_handler.run());
        let consensus_fut = starknet_consensus::run_consensus(
            context,
            active_height,
            observer_height,
            self.config.consensus_config.validator_id,
            self.config.consensus_config.startup_delay,
            self.config.consensus_config.timeouts.clone(),
            self.config.consensus_config.sync_retry_interval,
            votes_broadcast_channels.into(),
            inbound_internal_receiver,
        );

        tokio::select! {
            consensus_result = consensus_fut => {
                match consensus_result {
                    Ok(_) => panic!("Consensus task finished unexpectedly"),
                    Err(e) => Err(e),
                }
            },
            network_result = network_task => {
                panic!("Consensus' network task finished unexpectedly: {:?}", network_result);
            }
            stream_handler_result = stream_handler_task => {
                panic!("Consensus' stream handler task finished unexpectedly: {:?}", stream_handler_result);
            }
        }
    }

    // Performs reverts to the batcher.
    async fn revert_batcher_blocks(&self, revert_up_to_and_including: BlockNumber) {
        // If we revert all blocks up to height X (including), the new height marker will be X.
        let batcher_height_marker = self
            .batcher_client
            .get_height()
            .await
            .expect("Failed to get batcher_height_marker from batcher")
            .height;

        // This function will panic if the revert fails.
        let revert_blocks_fn = move |height| async move {
            self.batcher_client
                .revert_block(RevertBlockInput { height })
                .await
                .expect("Failed to revert block at height {height} in the batcher");
        };

        revert_blocks_and_eternal_pending(
            batcher_height_marker,
            revert_up_to_and_including,
            revert_blocks_fn,
            "Batcher",
        )
        .await;
    }
}

pub fn create_consensus_manager(
    config: ConsensusManagerConfig,
    batcher_client: SharedBatcherClient,
    state_sync_client: SharedStateSyncClient,
    class_manager_client: SharedClassManagerClient,
) -> ConsensusManager {
    ConsensusManager::new(config, batcher_client, state_sync_client, class_manager_client)
}

#[async_trait]
impl ComponentStarter for ConsensusManager {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run()
            .await
            .unwrap_or_else(|e| panic!("Failed to start ConsensusManager component: {:?}", e))
    }
}
