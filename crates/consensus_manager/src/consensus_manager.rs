use std::any::type_name;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::{self, SendError};
use futures::future::Ready;
use futures::SinkExt;
use papyrus_consensus::types::ConsensusError;
use papyrus_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ConsensusMessage;
use starknet_batcher_types::communication::SharedBatcherClient;
use starknet_mempool_infra::component_definitions::ComponentStarter;
use starknet_mempool_infra::errors::ComponentError;
use tracing::{error, info};

use crate::config::ConsensusManagerConfig;

// TODO(Tsabary/Matan): Replace with actual consensus manager code.

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
        let context = SequencerConsensusContext::new(
            Arc::clone(&self.batcher_client),
            self.config.consensus_config.num_validators,
        );

        papyrus_consensus::run_consensus(
            context,
            self.config.consensus_config.start_height,
            self.config.consensus_config.validator_id,
            self.config.consensus_config.consensus_delay,
            self.config.consensus_config.timeouts.clone(),
            create_fake_network_channels(),
            futures::stream::pending(),
        )
        .await
    }
}

// Milestone 1:
// We want to only run 1 node (e.g. no network), implying the local node can reach a quorum
// alone and is always the proposer. Actually connecting to the network will require an external
// dependency.
fn create_fake_network_channels() -> BroadcastTopicChannels<ConsensusMessage> {
    let messages_to_broadcast_fn: fn(ConsensusMessage) -> Ready<Result<Vec<u8>, SendError>> =
        |_| todo!("messages_to_broadcast_sender should not be used");
    let messages_to_broadcast_sender = mpsc::channel(0).0.with(messages_to_broadcast_fn);
    let broadcasted_messages_receiver = mpsc::channel(0).1;
    let reported_messages_sender = mpsc::channel(0).0;
    let continue_propagation_sender = mpsc::channel(0).0;
    let network_channels = BroadcastTopicChannels {
        messages_to_broadcast_sender,
        broadcasted_messages_receiver: Box::new(broadcasted_messages_receiver),
        reported_messages_sender: Box::new(reported_messages_sender),
        continue_propagation_sender: Box::new(continue_propagation_sender),
    };
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
            error!("Error running component {}: {:?}", type_name::<Self>(), e);
            ComponentError::InternalComponentError
        })
    }
}
