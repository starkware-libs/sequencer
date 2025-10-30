//! Runs a node that stress tests the p2p communication of the network.
#![allow(clippy::as_conversions)]
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use clap::Parser;
use converters::METADATA_SIZE;
use metrics_exporter_prometheus::PrometheusBuilder;
use tokio_metrics::RuntimeMetricsReporterBuilder;
use tracing::Level;

#[cfg(test)]
mod converters_test;

mod args;
mod converters;
mod explore_config;
mod message_handling;
pub mod metrics;
mod network_channels;
mod stress_test_node;
mod utils;

use args::Args;
use stress_test_node::BroadcastNetworkStressTestNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        args.message_size_bytes.unwrap_or(*METADATA_SIZE) >= *METADATA_SIZE,
        "Message size must be at least {} bytes",
        *METADATA_SIZE
    );

    // Protocol-specific validation
    if let Err(validation_error) =
        args.network_protocol.validate_message_size(args.message_size_bytes)
    {
        panic!("{}", validation_error);
    }

    // Set up metrics
    let builder = PrometheusBuilder::new().with_http_listener(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        args.metric_port,
    )));

    builder.install().expect("Failed to install prometheus recorder/exporter");

    // Start the tokio runtime metrics reporter to automatically collect and export runtime metrics
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(1))
            .describe_and_run(),
    );

    // Create and run the stress test node
    let stress_test_node = BroadcastNetworkStressTestNode::new(args).await;
    stress_test_node.run().await
}
