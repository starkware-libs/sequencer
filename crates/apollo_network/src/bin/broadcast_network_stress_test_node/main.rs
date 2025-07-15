//! Runs a node that stress tests the p2p communication of the network.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::time::SystemTime;
use std::vec;

use apollo_network::network_manager::{
    BroadcastTopicChannels,
    BroadcastTopicClient,
    BroadcastTopicClientTrait,
    BroadcastTopicServer,
    NetworkManager,
};
use apollo_network::NetworkConfig;
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use clap::Parser;
use converters::{StressTestMessage, METADATA_SIZE};
use futures::future::join_all;
use futures::StreamExt;
use libp2p::gossipsub::{Sha256Topic, Topic};
use libp2p::Multiaddr;
use metrics::{counter, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio::time::Duration;
use tracing::{info, trace, Level};

mod converters;
mod utils;

lazy_static::lazy_static! {
    static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

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

    /// Set the verbosity level of the logger, the higher the more verbose
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
        message.time = SystemTime::now();
        // message.id = i;
        broadcast_topic_client.broadcast_message(message.clone()).await.unwrap();
        trace!("Sent message {i}: {:?}", message);
        counter!("sent_messages").increment(1);
        tokio::time::sleep(duration).await;
    }
}

fn receive_stress_test_message(
    message_result: Result<StressTestMessage, Infallible>,
    _metadata: BroadcastedMessageMetadata,
) {
    let end_time = SystemTime::now();

    let received_message = message_result.unwrap();
    let start_time = received_message.time;
    let duration = match end_time.duration_since(start_time) {
        Ok(duration) => duration,
        Err(_) => panic!("Got a negative duration, the clocks are not synced!"),
    };

    let delay_seconds = duration.as_secs_f64();
    let delay_micros = duration.as_micros().try_into().unwrap();

    // TODO(AndrewL): Concentrate all string metrics to constants in a different file
    counter!("message_received").increment(1);
    counter!(format!("message_received_from_{}", received_message.id)).increment(1);

    // TODO(AndrewL): This should be a historgram
    gauge!("message_received_delay_seconds").set(delay_seconds);
    gauge!(format!("message_received_delay_seconds_from_{}", received_message.id))
        .set(delay_seconds);

    counter!("message_received_delay_micros_sum").increment(delay_micros);
    counter!(format!("message_received_delay_micros_sum_from_{}", received_message.id))
        .increment(delay_micros);
    // TODO(AndrewL): Figure out what to log here
}

async fn receive_stress_test_messages(
    broadcasted_messages_receiver: BroadcastTopicServer<StressTestMessage>,
) {
    broadcasted_messages_receiver
        .for_each(|result| async {
            let (message_result, metadata) = result;
            tokio::task::spawn_blocking(|| receive_stress_test_message(message_result, metadata));
        })
        .await;
    unreachable!("BroadcastTopicServer stream should never terminate...");
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

    let level = match args.verbosity {
        0 => None,
        1 => Some(Level::ERROR),
        2 => Some(Level::WARN),
        3 => Some(Level::INFO),
        4 => Some(Level::DEBUG),
        _ => Some(Level::TRACE),
    };
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder().with_max_level(level).finish(),
    )
    .expect("Failed to set global default subscriber");

    println!("Starting network stress test with args:\n{args:?}");

    assert!(
        args.message_size_bytes >= METADATA_SIZE,
        "Message size must be at least {METADATA_SIZE} bytes"
    );

    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        args.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    let peer_private_key = create_peer_private_key(args.id);
    let peer_private_key_hex =
        peer_private_key.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    info!("Secret Key: {peer_private_key_hex:#?}");

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
    info!("My PeerId: {peer_id}");

    let network_channels = network_manager
        .register_broadcast_topic::<StressTestMessage>(TOPIC.clone(), args.buffer_size)
        .unwrap();
    let BroadcastTopicChannels { broadcasted_messages_receiver, broadcast_topic_client } =
        network_channels;

    let mut tasks = Vec::new();

    tasks.push(tokio::spawn(async move {
        // Start the network manager to handle incoming connections and messages.
        network_manager.run().await.unwrap();
        unreachable!("Network manager should not exit");
    }));

    tasks.push(tokio::spawn(async move {
        receive_stress_test_messages(broadcasted_messages_receiver).await;
        unreachable!("Broadcast topic receiver should not exit");
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
            info!("Test timeout after {e}");
        }
    }
}
