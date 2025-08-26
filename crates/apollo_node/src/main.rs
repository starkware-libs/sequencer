use std::env::args;

use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::set_global_allocator;
use apollo_node::servers::run_component_servers;
use apollo_node::utils::{create_node_modules, load_and_validate_config};
use tokio::time::Duration;
use tokio_metrics::RuntimeMetricsReporterBuilder;
use tracing::info;

set_global_allocator!();

// TODO(Tsabary): remove the hook definition after we transition to proper usage of task spawning.
fn set_exit_process_on_panic() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
}

/// Start the tokio runtime metrics reporter to automatically collect and export runtime metrics
fn setup_tokio_metrics() {
    tokio::spawn(
        RuntimeMetricsReporterBuilder::default()
            .with_interval(Duration::from_secs(10))
            .describe_and_run(),
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    setup_tokio_metrics();
    set_exit_process_on_panic();

    let config =
        load_and_validate_config(args().collect()).expect("Failed to load and validate config");

    // Clients are currently unused, but should not be dropped.
    let (_clients, servers) = create_node_modules(&config).await;

    info!("START_UP: Starting components!");
    run_component_servers(servers).await;

    // TODO(Tsabary): Add graceful shutdown.
    Ok(())
}
