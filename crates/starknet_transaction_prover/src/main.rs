//! CLI for the Starknet transaction prover.

#[cfg(not(feature = "stwo_proving"))]
fn main() {
    eprintln!("The `starknet_transaction_prover` binary requires the `stwo_proving` feature.");
    std::process::exit(1);
}

#[cfg(feature = "stwo_proving")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use anyhow::Context;
    use clap::Parser;
    use starknet_transaction_prover::server::config::{
        CliArgs,
        LogFormat,
        ServiceConfig,
        TransportMode,
    };
    use starknet_transaction_prover::server::cors::{build_cors_layer, cors_mode};
    use starknet_transaction_prover::server::log_redact::redact_url_host;
    use starknet_transaction_prover::server::metrics::install_exporter;
    use starknet_transaction_prover::server::panic::install_panic_hook;
    use starknet_transaction_prover::server::rpc_api::ProvingRpcServer;
    use starknet_transaction_prover::server::rpc_impl::ProvingRpcServerImpl;
    use starknet_transaction_prover::server::{
        start_server,
        MetricsLayer,
        OhttpJsonrpseeLayer,
        OHTTP_JSONRPSEE_BODY_BUILDER,
    };
    use tokio::signal::unix::{signal, SignalKind};
    use tower_ohttp::OhttpGateway;
    use tracing::{info, warn};
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let args = CliArgs::parse();

    // TODO(Avi): Revisit the starknet_transaction_prover=debug default once the service stabilizes.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,starknet_transaction_prover=debug,privacy_prove=info")
    });
    let registry = tracing_subscriber::registry().with(filter);
    match args.log_format {
        LogFormat::Json => registry.with(fmt::layer().json()).init(),
        LogFormat::Text => registry.with(fmt::layer()).init(),
    }

    // Install after tracing init so the hook's `error!` macro reaches the
    // subscriber. A panic before this line still hits the default stderr
    // handler.
    install_panic_hook();

    let config = ServiceConfig::from_args(args)?;

    // Install Prometheus exporter and emit `prover_build_info` before binding
    // so a scrape during slow startup still returns the build identity.
    let prometheus_handle =
        install_exporter(env!("CARGO_PKG_VERSION"), option_env!("GIT_SHA").unwrap_or("unknown"))
            .context("Failed to install Prometheus exporter")?;
    let metrics_layer = Some(MetricsLayer::new(prometheus_handle));

    // Startup banner — version + chain id + redacted RPC host only. No URLs
    // with userinfo, no fee token address, no TLS paths, no tx data.
    info!(
        version = env!("CARGO_PKG_VERSION"),
        git_sha = option_env!("GIT_SHA").unwrap_or("unknown"),
        chain_id = %config.prover_config.chain_id,
        rpc_node_host = %redact_url_host(&config.prover_config.rpc_node_url),
        validate_zero_fee_fields = config.prover_config.validate_zero_fee_fields,
        blocking_check_enabled = config.prover_config.blocking_check_url.is_some(),
        blocking_check_fail_open = config.prover_config.blocking_check_fail_open,
        ohttp_enabled = config.ohttp_enabled,
        "Starting Starknet transaction prover."
    );

    // Build and start the JSON-RPC server.
    let rpc_impl = ProvingRpcServerImpl::from_config(&config);
    let addr = SocketAddr::new(config.ip, config.port);
    let cors_layer = build_cors_layer(&config.cors_allow_origin)?;

    // Initialize OHTTP gateway if enabled.
    let ohttp_layer: Option<OhttpJsonrpseeLayer> = if config.ohttp_enabled {
        let gateway = OhttpGateway::from_env().context("Failed to initialize OHTTP gateway")?;
        info!("OHTTP envelope encryption enabled");
        Some(OhttpJsonrpseeLayer::new(
            Arc::new(gateway),
            usize::try_from(config.max_request_body_size).unwrap(),
            config.ohttp_key_cache_max_age_secs,
            OHTTP_JSONRPSEE_BODY_BUILDER,
        ))
    } else {
        None
    };

    let scheme = match &config.transport {
        TransportMode::Http => "http",
        TransportMode::Https { .. } => "https",
    };

    let (local_addr, server_handle) = start_server(
        addr,
        &config.transport,
        rpc_impl.into_rpc().into(),
        config.max_connections,
        config.max_request_body_size,
        cors_layer,
        ohttp_layer,
        metrics_layer,
    )
    .await?;

    info!(
        local_address = %local_addr,
        scheme,
        max_concurrent_requests = config.max_concurrent_requests,
        max_connections = config.max_connections,
        cors_mode = cors_mode(&config.cors_allow_origin),
        cors_allow_origin = ?config.cors_allow_origin,
        ohttp_enabled = config.ohttp_enabled,
        "JSON-RPC proving server is running."
    );

    // Bridge SIGTERM/SIGINT into jsonrpsee's `ServerHandle::stop` so
    // container teardown becomes visible in logs. Both handlers are
    // installed eagerly: if one fails, we still want the other to drive
    // a graceful shutdown rather than silently dropping it.
    let sigterm = signal(SignalKind::terminate())
        .inspect_err(|err| warn!(error = %err, "Failed to install SIGTERM handler"))
        .ok();
    let sigint = signal(SignalKind::interrupt())
        .inspect_err(|err| warn!(error = %err, "Failed to install SIGINT handler"))
        .ok();
    let shutdown_handle = server_handle.clone();
    tokio::spawn(async move {
        let (mut sigterm, mut sigint) = (sigterm, sigint);
        let signal_name = match (&mut sigterm, &mut sigint) {
            (Some(t), Some(i)) => tokio::select! {
                _ = t.recv() => "SIGTERM",
                _ = i.recv() => "SIGINT",
            },
            (Some(t), None) => {
                t.recv().await;
                "SIGTERM"
            }
            (None, Some(i)) => {
                i.recv().await;
                "SIGINT"
            }
            (None, None) => return,
        };
        info!(event = "shutdown_started", signal = signal_name, "Shutting down JSON-RPC server.");
        if let Err(err) = shutdown_handle.stop() {
            warn!(error = %err, "Failed to stop JSON-RPC server cleanly");
        }

        // Stay live for a second signal and force-exit. Tokio's OS-level
        // signal handler keeps intercepting SIGTERM/SIGINT even after the
        // first one fires (tokio-rs/tokio#7905); if we let our Signal
        // instances drop, a second Ctrl+C would be silently swallowed and
        // a stuck graceful-shutdown could only be killed with SIGKILL.
        // Re-await the already-registered handlers and exit non-zero on
        // the second hit so an operator can always reclaim the process.
        match (&mut sigterm, &mut sigint) {
            (Some(t), Some(i)) => {
                tokio::select! {
                    _ = t.recv() => {},
                    _ = i.recv() => {},
                }
            }
            (Some(t), None) => {
                t.recv().await;
            }
            (None, Some(i)) => {
                i.recv().await;
            }
            (None, None) => return,
        }
        warn!(event = "force_exit", "Received second termination signal; forcing exit.");
        std::process::exit(1);
    });

    server_handle.stopped().await;
    info!(event = "shutdown_complete", "JSON-RPC server stopped.");
    Ok(())
}
