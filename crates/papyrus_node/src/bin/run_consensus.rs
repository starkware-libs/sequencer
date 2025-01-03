//! Run a papyrus node with consensus enabled and the ability to simulate network issues for
//! consensus.
//!
//! Expects to receive 2 groupings of arguments:
//! 1. TestConfig - these are prefixed with `--test.` in the command.
//! 2. NodeConfig - any argument lacking the above prefix is assumed to be in NodeConfig.

use clap::Parser;
use futures::stream::StreamExt;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus::simulation_network_receiver::NetworkReceiver;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::BroadcastVoteChannel;
use papyrus_consensus_orchestrator::papyrus_consensus_context::PapyrusConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_node::bin_utils::build_configs;
use papyrus_node::run::{run, PapyrusResources, PapyrusTaskHandles, NETWORK_TOPIC};
use papyrus_p2p_sync::BUFFER_SIZE;
use papyrus_protobuf::consensus::{ProposalPart, StreamMessage};
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use tokio::task::JoinHandle;

/// Test configuration for consensus.
#[derive(Parser, Debug, Clone, PartialEq)]
pub struct TestConfig {
    #[arg(long = "cache_size", help = "The cache size for the test network receiver.")]
    pub cache_size: usize,
    #[arg(
        long = "random_seed",
        help = "The random seed for the test simulation to ensure repeatable test results."
    )]
    pub random_seed: u64,
    #[arg(long = "drop_probability", help = "The probability of dropping a message.")]
    pub drop_probability: f64,
    #[arg(long = "invalid_probability", help = "The probability of sending an invalid message.")]
    pub invalid_probability: f64,
    #[arg(long = "sync_topic", help = "The network topic for sync messages.")]
    pub sync_topic: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            cache_size: 1000,
            random_seed: 0,
            drop_probability: 0.0,
            invalid_probability: 0.0,
            sync_topic: "consensus_test_sync".to_string(),
        }
    }
}

fn build_consensus(
    consensus_config: ConsensusConfig,
    test_config: TestConfig,
    storage_reader: StorageReader,
    network_manager: &mut NetworkManager,
) -> anyhow::Result<Option<JoinHandle<anyhow::Result<()>>>> {
    let network_channels = network_manager.register_broadcast_topic(
        Topic::new(consensus_config.network_topic.clone()),
        BUFFER_SIZE,
    )?;
    let proposal_network_channels: BroadcastTopicChannels<StreamMessage<ProposalPart>> =
        network_manager.register_broadcast_topic(Topic::new(NETWORK_TOPIC), BUFFER_SIZE)?;
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = proposal_network_channels;
    let (outbound_internal_sender, inbound_internal_receiver, stream_handler_task_handle) =
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);
    // TODO(matan): connect this to an actual channel.
    let sync_channels = network_manager
        .register_broadcast_topic(Topic::new(test_config.sync_topic.clone()), BUFFER_SIZE)?;
    let context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        network_channels.broadcast_topic_client.clone(),
        outbound_internal_sender,
        consensus_config.num_validators,
        Some(sync_channels.broadcast_topic_client),
    );
    let sync_receiver =
        sync_channels.broadcasted_messages_receiver.map(|(vote, _report_sender)| {
            BlockNumber(vote.expect("Sync channel should never have errors").height)
        });
    let network_receiver = NetworkReceiver::new(
        network_channels.broadcasted_messages_receiver,
        test_config.cache_size,
        test_config.random_seed,
        test_config.drop_probability,
        test_config.invalid_probability,
    );
    let broadcast_vote_channels = BroadcastVoteChannel {
        broadcasted_messages_receiver: Box::new(network_receiver),
        broadcast_topic_client: network_channels.broadcast_topic_client,
    };

    let consensus_task = papyrus_consensus::run_consensus(
        context,
        consensus_config.start_height,
        consensus_config.start_height,
        consensus_config.validator_id,
        consensus_config.consensus_delay,
        consensus_config.timeouts.clone(),
        broadcast_vote_channels,
        inbound_internal_receiver,
        sync_receiver,
    );

    Ok(Some(tokio::spawn(async move {
        tokio::select! {
            stream_handler_result = stream_handler_task_handle => {
                match stream_handler_result {
                    Ok(()) => Err(anyhow::anyhow!("Stream handler task completed")),
                    Err(e) => Err(anyhow::anyhow!("Stream handler task failed: {:?}", e)),
                }
            },
            consensus_result = consensus_task => {
                match consensus_result {
                    Ok(()) => Err(anyhow::anyhow!("Consensus task completed")),
                    Err(e) => Err(anyhow::anyhow!("Consensus task failed: {:?}", e)),
                }
            },
        }
    })))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (test_config, node_config) = build_configs::<TestConfig>("--test.")?;

    let mut resources = PapyrusResources::new(&node_config)?;

    let consensus_handle = build_consensus(
        node_config.consensus.clone().unwrap(),
        test_config,
        resources.storage_reader.clone(),
        resources.maybe_network_manager.as_mut().unwrap(),
    )?;
    let tasks = PapyrusTaskHandles { consensus_handle, ..Default::default() };

    run(node_config, resources, tasks).await
}
