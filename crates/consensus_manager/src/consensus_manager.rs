use std::any::type_name;
use std::sync::Arc;

use async_trait::async_trait;
use futures::channel::mpsc::{self, SendError};
use futures::future::Ready;
use futures::SinkExt;
use libp2p::PeerId;
use papyrus_consensus::types::{BroadcastConsensusMessageChannel, ConsensusError};
use papyrus_consensus_orchestrator::sequencer_consensus_context::SequencerConsensusContext;
use papyrus_network::network_manager::BroadcastTopicClient;
use papyrus_network_types::network_types::BroadcastedMessageManager;
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
fn create_fake_network_channels() -> BroadcastConsensusMessageChannel {
    let messages_to_broadcast_fn: fn(ConsensusMessage) -> Ready<Result<Vec<u8>, SendError>> =
        |_| todo!("messages_to_broadcast_sender should not be used");
    let reported_messages_sender_fn: fn(
        BroadcastedMessageManager,
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
