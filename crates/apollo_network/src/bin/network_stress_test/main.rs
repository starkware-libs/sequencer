use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::SystemTime;
use std::vec;

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    NetworkManager,
};
use apollo_network::NetworkConfig;
use clap::Parser;
use converters::{StressTestMessage, METADATA_SIZE};
use futures::future::join_all;
use libp2p::gossipsub::Topic;
use libp2p::Multiaddr;
use metrics::counter;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::time::Duration;

mod converters;
mod utils;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    id: usize,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env, default_value_t = 2000)]
    metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long, default_value_t = 10000)]
    p2p_port: u16,

    /// The address to the bootstrap peer
    #[arg(long, env)]
    bootstrap: Option<String>,

    /// Set the verbosity level of the logger
    #[arg(short, long, env, default_value_t = 0)]
    verbosity: u8,

    /// Buffer size for the broadcast topic
    // Default from crates/apollo_consensus_manager/src/config.rs
    #[arg(short, long, env, default_value_t = 10000)]
    buffer_size: usize,

    /// Size of StressTestMessage
    #[arg(short, long, env, default_value_t = 1 << 10)]
    message_size_bytes: usize,

    /// The time to sleep between broadcasts of StressTestMessage in milliseconds
    #[arg(long, env, default_value_t = 1)]
    heartbeat_millis: u64,

    /// Maximum duration in seconds to run the node for
    #[arg(short, long, env, default_value_t = 3_600)]
    timeout: u64,
}

fn log(message: &str, args: &Args, level: u8) {
    if args.verbosity >= level {
        println!("[{}] {}", args.id, message);
    }
}

async fn send_stress_test_messages(
    mut broadcast_topic_client: BroadcastTopicClient<StressTestMessage>,
    args: &Args,
    peer_id: String,
) {
    let mut message = StressTestMessage::new(
        args.id.try_into().unwrap(),
        vec![0; args.message_size_bytes - METADATA_SIZE],
        peer_id.clone(),
    );
    let duration = Duration::from_millis(args.heartbeat_millis);

    for i in 0.. {
        tokio::time::sleep(duration).await;
        message.time = SystemTime::now();
        // message.id = i;
        broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();
        log(&format!("Sent message {i}"), args, 1);
        counter!("sent_messages").increment(1);
    }
}

fn create_peer_private_key(peer_index: usize) -> [u8; 32] {
    let peer_index: u64 = peer_index.try_into().expect("Failed converting usize to u64");
    let array = peer_index.to_le_bytes();
    assert_eq!(array.len(), 8);
    let mut private_key = [0u8; 32];
    private_key[0..8].copy_from_slice(&array);
    private_key
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::LOCALHOST,
        args.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    let peer_private_key = create_peer_private_key(args.id);
    log(&format!("Secret Key: {:#?}", peer_private_key), &args, 1);

    let mut network_config = NetworkConfig {
        port: args.p2p_port,
        secret_key: Some(peer_private_key.to_vec()),
        ..Default::default()
    };
    if let Some(peer) = &args.bootstrap {
        let bootstrap_peer: Multiaddr = Multiaddr::from_str(peer).unwrap();
        network_config.bootstrap_peer_multiaddr = Some(vec![bootstrap_peer]);
    }

    let mut network_manager = NetworkManager::new(network_config, None, None);

    let peer_id = network_manager.get_local_peer_id();
    log(&format!("My PeerId: {}", peer_id), &args, 1);

    let network_channels = network_manager
        .register_broadcast_topic::<StressTestMessage>(
            Topic::new("stress_test_topic".to_string()),
            args.buffer_size,
        )
        .unwrap();
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        network_channels;

    let mut tasks = Vec::new();

    tasks.push(tokio::spawn(async move {
        // Start the network manager to handle incoming connections and messages.
        network_manager.run().await.unwrap();
        unreachable!("Network manager should not exit");
    }));

    let args_clone = args.clone();
    tasks.push(tokio::spawn(async move {
        send_stress_test_messages(broadcast_topic_client, &args_clone, peer_id).await;
        unreachable!("Broadcast topic client should not exit");
    }));

    let test_timeout = Duration::from_secs(args.timeout);
    match tokio::time::timeout(test_timeout, join_all(tasks.into_iter())).await {
        Ok(_) => unreachable!(),
        Err(e) => {
            log(&format!("Test timeout after {}", e), &args, 1);
        }
    }
}
