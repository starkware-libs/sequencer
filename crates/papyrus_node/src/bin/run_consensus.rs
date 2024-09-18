use clap::Parser;
use futures::stream::StreamExt;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus_orchestrator::papyrus_consensus_context::PapyrusConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_node::config::test_utils::build_configs;
use papyrus_node::run::{run, PapyrusResources, PapyrusTaskHandles};
use papyrus_p2p_sync::BUFFER_SIZE;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use tokio::task::JoinHandle;
use tracing::info;

#[derive(Parser, Debug)]
struct TestConfig {
    #[arg(long)]
    pub network_topic: String,

    #[arg(long)]
    pub num: i32,

    #[arg(long)]
    pub num_validators: Option<u64>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self { network_topic: String::from(""), num: -1, num_validators: None }
    }
}

fn build_consensus(
    config: &ConsensusConfig,
    storage_reader: StorageReader,
    network_manager: &mut NetworkManager,
) -> anyhow::Result<Option<JoinHandle<anyhow::Result<()>>>> {
    let config = config.clone();
    let Some(test_config) = config.test.as_ref() else {
        info!("Using the default consensus implementation.");
        return Ok(None);
    };

    let network_channels = network_manager
        .register_broadcast_topic(Topic::new(config.network_topic.clone()), BUFFER_SIZE)?;
    let BroadcastTopicChannels { messages_to_broadcast_sender, broadcast_client_channels } =
        network_channels;
    // TODO(matan): connect this to an actual channel.
    let sync_channels = network_manager
        .register_broadcast_topic(Topic::new(test_config.sync_topic.clone()), BUFFER_SIZE)?;
    let context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        messages_to_broadcast_sender,
        config.num_validators,
        Some(sync_channels.messages_to_broadcast_sender),
    );
    let sync_receiver = sync_channels.broadcast_client_channels.map(|(vote, _report_sender)| {
        BlockNumber(vote.expect("Sync channel should never have errors").height)
    });
    Ok(Some(tokio::spawn(async move {
        Ok(papyrus_consensus::run_consensus(
            context,
            config.start_height,
            config.validator_id,
            config.consensus_delay,
            config.timeouts.clone(),
            broadcast_client_channels,
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
        node_config.consensus.as_ref().unwrap(),
        resources.storage_reader.clone(),
        resources.maybe_network_manager.as_mut().unwrap(),
    )?;
    run(node_config, resources, tasks).await
}
