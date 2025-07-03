use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use clap::Parser;
use metrics_exporter_prometheus::PrometheusBuilder;
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
}
