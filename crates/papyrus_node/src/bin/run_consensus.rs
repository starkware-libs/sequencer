use clap::Parser;
use futures::stream::StreamExt;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus::simulation_network_receiver::NetworkReceiver;
use papyrus_consensus_orchestrator::papyrus_consensus_context::PapyrusConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_node::config::test_utils::build_configs;
use papyrus_node::run::{run, PapyrusResources, PapyrusTaskHandles};
use papyrus_p2p_sync::BUFFER_SIZE;
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
    // TODO(matan): connect this to an actual channel.
    let sync_channels = network_manager
        .register_broadcast_topic(Topic::new(test_config.sync_topic.clone()), BUFFER_SIZE)?;
    let context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        network_channels.messages_to_broadcast_sender.clone(),
        consensus_config.num_validators,
        Some(sync_channels.messages_to_broadcast_sender),
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
    let broadcast_channels = BroadcastTopicChannels {
        messages_to_broadcast_sender: network_channels.messages_to_broadcast_sender,
        broadcasted_messages_receiver: Box::new(network_receiver),
        reported_messages_sender: network_channels.reported_messages_sender,
        continue_propagation_sender: network_channels.continue_propagation_sender,
    };

    Ok(Some(tokio::spawn(async move {
        Ok(papyrus_consensus::run_consensus(
            context,
            consensus_config.start_height,
            consensus_config.validator_id,
            consensus_config.consensus_delay,
            consensus_config.timeouts.clone(),
            broadcast_channels,
            sync_receiver,
        )
        .await?)
    })))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (test_config, node_config) = build_configs::<TestConfig>()?;
    dbg!(&test_config);

    let mut resources = PapyrusResources::new(&node_config)?;
    let mut tasks = PapyrusTaskHandles::default();

    tasks.consensus_handle = build_consensus(
        node_config.consensus.clone().unwrap(),
        test_config,
        resources.storage_reader.clone(),
        resources.maybe_network_manager.as_mut().unwrap(),
    )?;
    run(node_config, resources, tasks).await
}
