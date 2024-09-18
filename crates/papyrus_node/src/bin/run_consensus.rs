use std::env::args;

use futures::stream::StreamExt;
use papyrus_config::ConfigError;
use papyrus_consensus::config::ConsensusConfig;
use papyrus_consensus_orchestrator::papyrus_consensus_context::PapyrusConsensusContext;
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::{BroadcastTopicChannels, NetworkManager};
use papyrus_node::config::NodeConfig;
use papyrus_node::run::{run, PapyrusTaskHandles, PapyrusUtilities};
use papyrus_p2p_sync::BUFFER_SIZE;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use tokio::task::JoinHandle;
use tracing::info;

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
    let config = NodeConfig::load_and_process(args().collect());
    if let Err(ConfigError::CommandInput(clap_err)) = config {
        clap_err.exit();
    }
    let config = config?;

    let mut utils = PapyrusUtilities::new(&config)?;
    let mut tasks = PapyrusTaskHandles::default();

    tasks.consensus_handle = build_consensus(
        config.consensus.as_ref().unwrap(),
        utils.storage_reader.clone(),
        utils.maybe_network_manager.as_mut().unwrap(),
    )?;
    run(config, utils, tasks).await
}
