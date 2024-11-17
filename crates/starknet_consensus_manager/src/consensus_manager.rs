use std::any::type_name;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::{self, SendError};
use futures::future::Ready;
use futures::SinkExt;
use libp2p::PeerId;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::{BroadcastConsensusMessageChannel, ConsensusError};
use papyrus_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    NetworkManager,
};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::consensus::{ConsensusMessage, ProposalPart, StreamMessage};
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sequencer_infra::errors::ComponentError;
use tracing::{error, info};

use crate::config::ConsensusManagerConfig;

// TODO(Dan, Guy): move to config.
pub const BROADCAST_BUFFER_SIZE: usize = 100;
pub const NETWORK_TOPIC: &str = "consensus_proposals";
// TODO(guyn): remove this once we have integrated streaming.
pub const NETWORK_TOPIC2: &str = "streamed_consensus_proposals";

#[derive(Clone)]
pub struct ConsensusManager {
    pub config: ConsensusManagerConfig,
    pub batcher_client: SharedBatcherClient,
}

impl ConsensusManager {
    pub fn new(config: ConsensusManagerConfig, batcher_client: SharedBatcherClient) -> Self {
        Self { config, batcher_client }
    }

    pub async fn run(&self) -> Result<(), ConsensusError> {
        let mut network_manager =
            NetworkManager::new(self.config.consensus_config.network_config.clone(), None);

        // TODO(guyn): remove this channel once we have integrated streaming.
        let old_proposals_broadcast_channels = network_manager
            .register_broadcast_topic::<ProposalPart>(
                Topic::new(NETWORK_TOPIC),
                BROADCAST_BUFFER_SIZE,
            )
            .expect("Failed to register broadcast topic");

        let proposals_broadcast_channels = network_manager
            .register_broadcast_topic::<StreamMessage<ProposalPart>>(
                Topic::new(NETWORK_TOPIC2),
                BROADCAST_BUFFER_SIZE,
            )
            .expect("Failed to register broadcast topic");
        let BroadcastTopicChannels {
            broadcasted_messages_receiver: inbound_network_receiver,
            broadcast_topic_client: outbound_network_sender,
        } = proposals_broadcast_channels;

        let (outbound_internal_sender, inbound_internal_receiver) =
            StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

        let context = SequencerConsensusContext::new(
            Arc::clone(&self.batcher_client),
            old_proposals_broadcast_channels.broadcast_topic_client.clone(),
            outbound_internal_sender,
            self.config.consensus_config.num_validators,
        );

        let mut network_handle = tokio::task::spawn(network_manager.run());
        let consensus_task = papyrus_consensus::run_consensus(
            context,
            self.config.consensus_config.start_height,
            self.config.consensus_config.validator_id,
            self.config.consensus_config.consensus_delay,
            self.config.consensus_config.timeouts.clone(),
            create_fake_network_channels(),
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
        }
    }
}

// Milestone 1:
// We want to only run 1 node (e.g. no network), implying the local node can reach a quorum
// alone and is always the proposer. Actually connecting to the network will require an external
// dependency.
fn create_fake_network_channels() -> BroadcastConsensusMessageChannel {
    let messages_to_broadcast_fn: fn(ConsensusMessage) -> Ready<Result<Vec<u8>, SendError>> =
        |_| todo!("messages_to_broadcast_sender should not be used");
    let reported_messages_sender_fn: fn(
        BroadcastedMessageMetadata,
    ) -> Ready<Result<PeerId, SendError>> =
        |_| todo!("messages_to_broadcast_sender should not be used");
    let broadcast_topic_client = BroadcastTopicClient::new(
        mpsc::channel(0).0.with(messages_to_broadcast_fn),
        mpsc::channel(0).0.with(reported_messages_sender_fn),
        mpsc::channel(0).0,
    );
    BroadcastConsensusMessageChannel {
        broadcasted_messages_receiver: Box::new(futures::stream::pending()),
        broadcast_topic_client,
    }
}

pub fn create_consensus_manager(
    config: ConsensusManagerConfig,
    batcher_client: SharedBatcherClient,
) -> ConsensusManager {
    ConsensusManager::new(config, batcher_client)
}

#[async_trait]
impl ComponentStarter for ConsensusManager {
    async fn start(&mut self) -> Result<(), ComponentError> {
        info!("Starting component {}.", type_name::<Self>());
        self.run().await.map_err(|e| {
            error!("Error running component ConsensusManager: {:?}", e);
            ComponentError::InternalComponentError
        })
    }
}
