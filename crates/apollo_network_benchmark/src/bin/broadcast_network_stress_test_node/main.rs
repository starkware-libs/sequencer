//! Runs a node that stress tests the p2p communication of the network.

use clap::Parser;

#[cfg(test)]
mod message_test;

mod args;
mod message;

use args::Args;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _args = Args::parse();
    Ok(())
}
