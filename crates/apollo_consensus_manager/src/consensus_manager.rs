#[cfg(test)]
#[path = "consensus_manager_test.rs"]
mod consensus_manager_test;

use std::collections::HashMap;
use std::sync::Arc;

use apollo_batcher_types::batcher_types::RevertBlockInput;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_class_manager_types::transaction_converter::TransactionConverter;
use apollo_class_manager_types::SharedClassManagerClient;
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_consensus::stream_handler::StreamHandler;
use apollo_consensus::votes_threshold::QuorumType;
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_consensus_orchestrator::cende::CendeAmbassador;
use apollo_consensus_orchestrator::sequencer_consensus_context::{
    SequencerConsensusContext,
    SequencerConsensusContextDeps,
};
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use apollo_l1_gas_price_types::L1GasPriceProviderClient;
use apollo_network::gossipsub_impl::Topic;
use apollo_network::network_manager::metrics::{
    BroadcastNetworkMetrics,
    EventMetrics,
    NetworkMetrics,
};
use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicServer,
    NetworkManager,
};
use apollo_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage, Vote};
use apollo_reverts::revert_blocks_and_eternal_pending;
use apollo_signature_manager_types::SharedSignatureManagerClient;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use apollo_time::time::DefaultClock;
use async_trait::async_trait;
use futures::channel::mpsc;
use starknet_api::block::BlockNumber;
use tracing::{info, info_span, Instrument};

use crate::metrics::{
    register_metrics,
    CONSENSUS_NETWORK_EVENTS,
    CONSENSUS_NUM_BLACKLISTED_PEERS,
    CONSENSUS_NUM_CONNECTED_PEERS,
    CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    CONSENSUS_VOTES_NUM_DROPPED_MESSAGES,
    CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};

type ProposalStreamMessage = StreamMessage<ProposalPart, HeightAndRound>;
type ProposalStreamHandler = StreamHandler<
    ProposalPart,
    HeightAndRound,
    BroadcastTopicServer<ProposalStreamMessage>,
    BroadcastTopicClient<ProposalStreamMessage>,
>;

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
    pub state_sync_client: SharedStateSyncClient,
    pub class_manager_client: SharedClassManagerClient,
    pub signature_manager_client: SharedSignatureManagerClient,
    pub config_manager_client: SharedConfigManagerClient,
    l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
}

impl ConsensusManager {
    pub fn new(
        config: ConsensusManagerConfig,
        batcher_client: SharedBatcherClient,
        state_sync_client: SharedStateSyncClient,
        class_manager_client: SharedClassManagerClient,
        signature_manager_client: SharedSignatureManagerClient,
        config_manager_client: SharedConfigManagerClient,
        l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
    ) -> Self {
        Self {
            config,
            batcher_client,
            state_sync_client,
            class_manager_client,
            signature_manager_client,
            config_manager_client,
            l1_gas_price_provider,
        }
    }

    pub async fn run(&self) {
        if self.config.revert_config.should_revert {
            self.revert_batcher_blocks(self.config.revert_config.revert_up_to_and_including).await;
        }

        let mut network_manager = self.create_network_manager();
        let (proposals_broadcast_channels, votes_broadcast_channels) =
            self.register_broadcast_topics(&mut network_manager);

        let (inbound_internal_sender, inbound_internal_receiver) =
            mpsc::channel(self.config.stream_handler_config.channel_buffer_capacity);
        let (outbound_internal_sender, outbound_internal_receiver) =
            mpsc::channel(self.config.stream_handler_config.channel_buffer_capacity);

        let stream_handler = self.create_stream_handler(
            proposals_broadcast_channels,
            inbound_internal_sender,
            outbound_internal_receiver,
        );

        let consensus_context = self.create_sequencer_consensus_context(
            &votes_broadcast_channels,
            outbound_internal_sender,
        );

        let current_height =
            self.batcher_client.get_height().await.expect("Failed to get height from batcher");
        let run_consensus_args = self.create_run_consensus_args(current_height.height);

        let network_task =
            tokio::spawn(network_manager.run().instrument(info_span!("[Consensus network]")));
        let stream_handler_task = tokio::spawn(stream_handler.run());

        let consensus_fut = apollo_consensus::run_consensus(
            run_consensus_args,
            consensus_context,
            votes_broadcast_channels.into(),
            inbound_internal_receiver,
        );

        tokio::select! {
            consensus_result = consensus_fut =>
                panic!("Consensus task finished unexpectedly: {:?}", consensus_result),
            network_result = network_task =>
                panic!("Consensus' network task finished unexpectedly: {:?}", network_result),
            stream_handler_result = stream_handler_task =>
                panic!("Consensus' stream handler task finished unexpectedly: {:?}", stream_handler_result)
        }
    }

    fn create_network_manager(&self) -> NetworkManager {
        let mut broadcast_metrics_by_topic = HashMap::new();
        broadcast_metrics_by_topic.insert(
            Topic::new(self.config.votes_topic.clone()).hash(),
            BroadcastNetworkMetrics {
                num_sent_broadcast_messages: CONSENSUS_VOTES_NUM_SENT_MESSAGES,
                num_received_broadcast_messages: CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
                num_dropped_broadcast_messages: CONSENSUS_VOTES_NUM_DROPPED_MESSAGES,
            },
        );
        broadcast_metrics_by_topic.insert(
            Topic::new(self.config.proposals_topic.clone()).hash(),
            BroadcastNetworkMetrics {
                num_sent_broadcast_messages: CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
                num_received_broadcast_messages: CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
                num_dropped_broadcast_messages: CONSENSUS_PROPOSALS_NUM_DROPPED_MESSAGES,
            },
        );
        let network_manager_metrics = Some(NetworkMetrics {
            num_connected_peers: CONSENSUS_NUM_CONNECTED_PEERS,
            num_blacklisted_peers: CONSENSUS_NUM_BLACKLISTED_PEERS,
            broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
            sqmr_metrics: None,
            event_metrics: Some(EventMetrics { event_counter: CONSENSUS_NETWORK_EVENTS }),
        });

        NetworkManager::new(self.config.network_config.clone(), None, network_manager_metrics)
    }

    fn create_stream_handler(
        &self,
        proposals_broadcast_channels: BroadcastTopicChannels<ProposalStreamMessage>,
        inbound_internal_sender: mpsc::Sender<mpsc::Receiver<ProposalPart>>,
        outbound_internal_receiver: mpsc::Receiver<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    ) -> ProposalStreamHandler {
        let BroadcastTopicChannels {
            broadcasted_messages_receiver: inbound_network_receiver,
            broadcast_topic_client: outbound_network_sender,
        } = proposals_broadcast_channels;

        StreamHandler::new(
            self.config.stream_handler_config.clone(),
            inbound_internal_sender,
            inbound_network_receiver,
            outbound_internal_receiver,
            outbound_network_sender,
        )
    }

    fn register_broadcast_topics(
        &self,
        network_manager: &mut NetworkManager,
    ) -> (BroadcastTopicChannels<ProposalStreamMessage>, BroadcastTopicChannels<Vote>) {
        let proposals_broadcast_channels = network_manager
            .register_broadcast_topic::<ProposalStreamMessage>(
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

        (proposals_broadcast_channels, votes_broadcast_channels)
    }

    fn create_sequencer_consensus_context(
        &self,
        votes_broadcast_channels: &BroadcastTopicChannels<Vote>,
        outbound_internal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    ) -> SequencerConsensusContext {
        SequencerConsensusContext::new(
            self.config.context_config.clone(),
            SequencerConsensusContextDeps {
                transaction_converter: Arc::new(TransactionConverter::new(
                    Arc::clone(&self.class_manager_client),
                    self.config.context_config.chain_id.clone(),
                )),
                state_sync_client: Arc::clone(&self.state_sync_client),
                batcher: Arc::clone(&self.batcher_client),
                cende_ambassador: Arc::new(CendeAmbassador::new(
                    self.config.cende_config.clone(),
                    Arc::clone(&self.class_manager_client),
                )),
                l1_gas_price_provider: self.l1_gas_price_provider.clone(),
                clock: Arc::new(DefaultClock),
                outbound_proposal_sender: outbound_internal_sender,
                vote_broadcast_client: votes_broadcast_channels.broadcast_topic_client.clone(),
            },
        )
    }

    fn create_run_consensus_args(
        &self,
        current_height: BlockNumber,
    ) -> apollo_consensus::RunConsensusArguments {
        let observer_height = current_height;
        let active_height = if self.config.immediate_active_height == observer_height {
            // Setting `start_height` is only used to enable consensus starting immediately without
            // observing the first height. This means consensus may return to a height
            // it has already voted on, risking equivocation. This is only safe to do if we
            // restart all nodes at this height.
            observer_height
        } else {
            BlockNumber(observer_height.0 + 1)
        };
        let quorum_type = if self.config.assume_no_malicious_validators {
            QuorumType::Honest
        } else {
            QuorumType::Byzantine
        };
        apollo_consensus::RunConsensusArguments {
            start_active_height: active_height,
            start_observe_height: observer_height,
            consensus_config: self.config.consensus_manager_config.clone(),
            quorum_type,
            config_manager_client: Some(Arc::clone(&self.config_manager_client)),
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
    signature_manager_client: SharedSignatureManagerClient,
    config_manager_client: SharedConfigManagerClient,
    l1_gas_price_provider: Arc<dyn L1GasPriceProviderClient>,
) -> ConsensusManager {
    ConsensusManager::new(
        config,
        batcher_client,
        state_sync_client,
        class_manager_client,
        signature_manager_client,
        config_manager_client,
        l1_gas_price_provider,
    )
}

#[async_trait]
impl ComponentStarter for ConsensusManager {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        register_metrics();
        self.run().await;
    }
}
