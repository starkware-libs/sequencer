#[cfg(test)]
#[path = "consensus_manager_test.rs"]
mod consensus_manager_test;

use std::sync::Arc;

use apollo_reverts::revert_blocks_and_eternal_pending;
use async_trait::async_trait;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::metrics::{BroadcastNetworkMetrics, NetworkMetrics};
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage, Vote};
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::RevertBlockInput;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_consensus::stream_handler::StreamHandler;
use starknet_consensus::types::ConsensusError;
use starknet_consensus_orchestrator::cende::CendeAmbassador;
use starknet_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use starknet_infra_utils::type_name::short_type_name;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use starknet_sequencer_metrics::metric_definitions::{
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_NUM_SENT_MESSAGES,
};
use starknet_state_sync_types::communication::SharedStateSyncClient;
use tracing::{error, info};

use crate::config::ConsensusManagerConfig;

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
    pub state_sync_client: SharedStateSyncClient,
    pub class_manager_client: SharedClassManagerClient,
}

use std::sync::atomic::{AtomicU64, Ordering};

use starknet_batcher_types::batcher_types::{
    CentralObjects,
    DecisionReachedInput,
    DecisionReachedResponse,
    GetHeightResponse,
    GetProposalContent,
    GetProposalContentInput,
    GetProposalContentResponse,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::communication::{BatcherClient, BatcherClientResult};
use starknet_state_sync_types::state_sync_types::SyncBlock;

pub struct MockBatcherClient {
    pub height: AtomicU64,
}

#[async_trait]
impl BatcherClient for MockBatcherClient {
    async fn propose_block(&self, input: ProposeBlockInput) -> BatcherClientResult<()> {
        Ok(())
    }

    async fn get_height(&self) -> BatcherClientResult<GetHeightResponse> {
        Ok(GetHeightResponse { height: BlockNumber(self.height.load(Ordering::Relaxed)) })
    }

    async fn get_proposal_content(
        &self,
        input: GetProposalContentInput,
    ) -> BatcherClientResult<GetProposalContentResponse> {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        Ok(GetProposalContentResponse { content: GetProposalContent::Finished(Default::default()) })
    }

    async fn validate_block(&self, input: ValidateBlockInput) -> BatcherClientResult<()> {
        Ok(())
    }

    async fn send_proposal_content(
        &self,
        input: SendProposalContentInput,
    ) -> BatcherClientResult<SendProposalContentResponse> {
        match input.content {
            SendProposalContent::Txs(_) => {
                Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
            }
            SendProposalContent::Finish => Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(Default::default()),
            }),
            SendProposalContent::Abort => {
                Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
            }
        }
    }

    async fn start_height(&self, input: StartHeightInput) -> BatcherClientResult<()> {
        self.height.store(input.height.0, Ordering::Relaxed);
        Ok(())
    }

    async fn add_sync_block(&self, sync_block: SyncBlock) -> BatcherClientResult<()> {
        Ok(())
    }

    async fn decision_reached(
        &self,
        input: DecisionReachedInput,
    ) -> BatcherClientResult<DecisionReachedResponse> {
        Ok(DecisionReachedResponse {
            state_diff: Default::default(),
            l2_gas_used: Default::default(),
            central_objects: CentralObjects {
                execution_infos: Default::default(),
                bouncer_weights: Default::default(),
                compressed_state_diff: None,
            },
        })
    }

    async fn revert_block(&self, input: RevertBlockInput) -> BatcherClientResult<()> {
        self.height.compare_exchange(
            input.height.0,
            input.height.0 - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
        Ok(())
    }
}

impl ConsensusManager {
    pub fn new(
        config: ConsensusManagerConfig,
        batcher_client: SharedBatcherClient,
        state_sync_client: SharedStateSyncClient,
        class_manager_client: SharedClassManagerClient,
    ) -> Self {
        let batcher_client = Arc::new(MockBatcherClient { height: AtomicU64::new(0) });
        Self { config, batcher_client, state_sync_client, class_manager_client }
    }

    pub async fn run(&self) -> Result<(), ConsensusError> {
        // Sleep to let sync advance and not reach a race condition where sync returns None to
        // consensus even though the block exists in the network
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let height =
            self.state_sync_client.get_latest_block_number().await.unwrap().unwrap_or_default();
        self.batcher_client.start_height(StartHeightInput { height }).await;
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

        let (outbound_internal_sender, inbound_internal_receiver, mut stream_handler_task_handle) =
            StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

        let observer_height =
            self.batcher_client.get_height().await.map(|h| h.height).map_err(|e| {
                error!("Failed to get height from batcher: {:?}", e);
                ConsensusError::Other("Failed to get height from batcher".to_string())
            })?;
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
        );

        let mut network_handle = tokio::task::spawn(network_manager.run());
        let consensus_task = starknet_consensus::run_consensus(
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

    // Performs reverts to the batcher.
    async fn revert_batcher_blocks(&self, revert_up_to_and_including: BlockNumber) {
        // If we revert all blocks up to height X (including), the new height marker will be X.
        let batcher_height_marker = self
            .batcher_client
            .get_height()
            .await
            .expect("Failed to get height from batcher")
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
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.map_err(|e| {
            error!("Error running component ConsensusManager: {:?}", e);
            ComponentError::InternalComponentError
        })
    }
}
