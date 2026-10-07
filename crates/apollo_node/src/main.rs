use std::env::args;
use std::time::Duration;

use apollo_infra::metrics::{metrics_recorder, MetricsConfig};
use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::set_global_allocator;
use apollo_node::servers::run_component_servers;
use apollo_node::signal_handling::{
    handle_signals,
    GracefulShutdownBehavior,
    GracefulShutdownCallback,
};
use apollo_node::utils::create_node_modules;
use apollo_node_config::config_utils::load_and_validate_config;
use futures::FutureExt;
use starknet_api::block::StarknetVersion;
use starknet_api::versioned_constants_logic::set_effective_latest_version;
use tokio::time::sleep;
use tracing::{error, info, warn};

set_global_allocator!();

// TODO(Tsabary): remove the hook definition after we transition to proper usage of task spawning.
fn set_exit_process_on_panic() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
}

// In Echonet mode, overrides the process-wide effective latest Starknet version with the replayed
// network's version, so every component in this process looks up the same versioned constants.
async fn set_effective_starknet_version_from_echonet(recorder_url: &reqwest::Url) {
    match fetch_starknet_version(recorder_url).await {
        Ok(starknet_version) => {
            info!("Setting effective Starknet version from echonet: {starknet_version}");
            set_effective_latest_version(starknet_version);
        }
        Err(fetch_error) => warn!(
            "Failed to fetch the Starknet version from echonet; keeping the compile-time latest: \
             {fetch_error}"
        ),
    }
}

// Fetches the replayed network's Starknet version from the recorder.
async fn fetch_starknet_version(recorder_url: &reqwest::Url) -> Result<StarknetVersion, String> {
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
    let url = recorder_url
        .join("echonet/get_starknet_version")
        .map_err(|join_error| format!("invalid recorder URL: {join_error}"))?;
    let response = reqwest::Client::new()
        .get(url)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|request_error| format!("request failed: {request_error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let version_string = response
        .text()
        .await
        .map_err(|read_error| format!("failed to read response: {read_error}"))?;
    StarknetVersion::try_from(version_string.trim()).map_err(|parse_error| {
        format!("failed to parse Starknet version '{}': {parse_error}", version_string.trim())
    })
}

// Graceful shutdown duration limit is vendor dependent, 30 seconds in GKE. We set the graceful
// shutdown period to be of 20 seconds, which suffice for all purposes as well as keeping a margin
// from the vendor threshold.
const GRACEFUL_SHUTDOWN_DURATION_MS: u64 = 20_000;

// TODO(Tsabary): Do we need a return type for `main`?
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    let prometheus_handle = metrics_recorder(MetricsConfig::enabled());

    set_exit_process_on_panic();

    let cli_args: Vec<String> = args().collect();
    let config = load_and_validate_config(cli_args.clone(), true)
        .expect("Failed to load and validate config");

    if let Some(recorder_url) = config.echonet_recorder_url() {
        set_effective_starknet_version_from_echonet(recorder_url).await;
    }

    // Clients are currently unused, but should not be dropped.
    let (_clients, servers) = create_node_modules(&config, prometheus_handle, cli_args).await;

    let sigterm_cb: GracefulShutdownCallback = Box::new(|| {
        async move {
            info!("Graceful shutdown future started");
            sleep(Duration::from_millis(GRACEFUL_SHUTDOWN_DURATION_MS)).await;
            info!("Graceful shutdown future completed");
        }
        .boxed()
    });

    let graceful_shutdown = GracefulShutdownBehavior::new().with_sigterm(sigterm_cb);

    info!("START_UP: Starting components!");
    tokio::select! {
        _ = run_component_servers(servers) => {
            error!("Shutting down: Servers ended unexpectedly!");
        }
        _ = handle_signals(graceful_shutdown) => {
            error!("Shutting down: Signal received and logged");
        }
    }

    // TODO(Tsabary): Add graceful shutdown.
    Ok(())
}
