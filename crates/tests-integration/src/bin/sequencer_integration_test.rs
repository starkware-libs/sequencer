use anyhow::Ok;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::compilation::compile_node_with_status;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test for the sequencer node.");

    // Compile the node
    info!("Compiling sequencer node.");
    if !compile_node_with_status() {
        error!("Failed to compile the node");
    };

    info!("Integration test completed successfully <3.");
    Ok(())
}
