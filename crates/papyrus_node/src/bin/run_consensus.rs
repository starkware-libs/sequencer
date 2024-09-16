use std::env::args;

use clap::Parser;
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

// Test arguments passed on the command line are prefixed with `test.<ARG_NAME>`.
const TEST_ARG_PREFIX: &str = "--test.";

/// Split the elements of `input_args` into 2 groups:
/// 1. Those prefixed with "--test."
/// 2. Other.
///
/// Presumes input is: program_name (--flag_name value)*
pub fn split_args(input_args: Vec<String>) -> (Vec<String>, Vec<String>) {
    input_args[1..].chunks(2).fold(
        (vec![input_args[0].clone()], vec![input_args[0].clone()]),
        |(mut matching_args, mut mismatched_args), input_arg| {
            let (name, value) = (&input_arg[0], &input_arg[1]);
            // String leading `--` for comparison.
            if &name[..TEST_ARG_PREFIX.len()] == TEST_ARG_PREFIX {
                matching_args.push(format!("--{}", name[TEST_ARG_PREFIX.len()..].to_string()));
                matching_args.push(value.clone());
            } else {
                mismatched_args.push(name.clone());
                mismatched_args.push(value.clone());
            }
            (matching_args, mismatched_args)
        },
    )
}

/// Build both the node and test configs from the command line arguments.
pub fn build_configs<T: Parser + Default>() -> Result<(T, NodeConfig), ConfigError> {
    let input_args = args().collect::<Vec<_>>();
    let (test_input_args, node_input_args) = split_args(input_args);
    dbg!(&test_input_args, &node_input_args);

    let mut test_config = T::default();
    test_config.update_from(test_input_args.iter());

    let node_config = NodeConfig::load_and_process(node_input_args);
    if let Err(ConfigError::CommandInput(clap_err)) = node_config {
        clap_err.exit();
    }
    let node_config = node_config?;
    Ok((test_config, node_config))
}

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

    let mut utils = PapyrusUtilities::new(&node_config)?;
    let mut tasks = PapyrusTaskHandles::default();

    tasks.consensus_handle = build_consensus(
        node_config.consensus.as_ref().unwrap(),
        utils.storage_reader.clone(),
        utils.maybe_network_manager.as_mut().unwrap(),
    )?;
    run(node_config, utils, tasks).await
}
