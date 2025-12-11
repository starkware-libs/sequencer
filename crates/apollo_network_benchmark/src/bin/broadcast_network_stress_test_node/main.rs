//! Runs a node that stress tests the p2p communication of the network.
#![allow(clippy::as_conversions)]
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use clap::Parser;
use message::METADATA_SIZE;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio_metrics::RuntimeMetricsReporterBuilder;
use tracing::Level;

#[cfg(test)]
mod message_test;

mod explore_config;
mod handlers;
mod message;
mod message_index_detector;
pub mod metrics;
mod protocol;
mod stress_test_node;
mod system_metrics;

use apollo_network_benchmark::node_args::NodeArgs;
use stress_test_node::BroadcastNetworkStressTestNode;
use system_metrics::monitor_process_metrics;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = NodeArgs::parse();

    let level = match args.user.verbosity {
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
        args.user.message_size_bytes >= *METADATA_SIZE,
        "Message size must be at least {} bytes",
        *METADATA_SIZE
    );

    // Set up metrics
    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        args.runner.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    // Start the tokio runtime metrics reporter to automatically collect and export runtime metrics
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(1))
            .describe_and_run(),
    );

    // Start the process metrics monitoring task
    tokio::spawn(async {
        monitor_process_metrics(1).await;
    });

    // Create and run the stress test node
    let stress_test_node = BroadcastNetworkStressTestNode::new(args).await;
    stress_test_node.run().await
}
