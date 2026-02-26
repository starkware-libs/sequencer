use std::env::args;

use apollo_infra::metrics::{metrics_recorder, MetricsConfig};
use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::set_global_allocator;
use apollo_node::servers::run_component_servers;
use apollo_node::signal_handling::handle_signals;
use apollo_node::utils::create_node_modules;
use apollo_node_config::config_utils::load_and_validate_config;
use tracing::{error, info};

set_global_allocator!();

// TODO(Tsabary): remove the hook definition after we transition to proper usage of task spawning.
fn set_exit_process_on_panic() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
}

// TODO(Tsabary): Do we need a return type for `main`?
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    let prometheus_handle = metrics_recorder(MetricsConfig::enabled());

    set_exit_process_on_panic();

    let cli_args: Vec<String> = args().collect();
    let config = load_and_validate_config(cli_args.clone(), true)
        .expect("Failed to load and validate config");

    // Clients are currently unused, but should not be dropped.
    // Production node doesn't use bootstrap transactions - they're only for integration tests.
    let (_clients, servers) =
        create_node_modules(&config, prometheus_handle, cli_args, vec![]).await;

    info!("START_UP: Starting components!");
    tokio::select! {
        _ = run_component_servers(servers) => {
            error!("Shutting down: Servers ended unexpectedly!");
        }
        _ = handle_signals() => {
            error!("Shutting down: Signal received and logged");
        }
    }

    // TODO(Tsabary): Add graceful shutdown.
    Ok(())
}
