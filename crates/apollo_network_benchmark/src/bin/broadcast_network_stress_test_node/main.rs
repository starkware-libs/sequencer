//! Runs a node that stress tests the p2p communication of the network.

use clap::Parser;

#[cfg(test)]
mod message_test;

mod message;

use apollo_network_benchmark::node_args::NodeArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _args = NodeArgs::parse();
    Ok(())
}
