use std::env::args;

use apollo_infra_utils::set_global_allocator;
use apollo_sequencer_infra::trace_util::configure_tracing;
use apollo_sequencer_node::servers::run_component_servers;
use apollo_sequencer_node::utils::{create_node_modules, load_and_validate_config};
use tracing::info;

set_global_allocator!();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    let config =
        load_and_validate_config(args().collect()).expect("Failed to load and validate config");

    // Clients are currently unused, but should not be dropped.
    let (_clients, servers) = create_node_modules(&config).await;

    info!("Starting components!");
    run_component_servers(servers).await;

    // TODO(Tsabary): Add graceful shutdown.
    Ok(())
}
