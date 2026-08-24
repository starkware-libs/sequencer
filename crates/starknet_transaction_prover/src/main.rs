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
    use std::time::Duration;

    use anyhow::Context;
    use clap::Parser;
    use starknet_transaction_prover::server::config::{
        CliArgs,
        LogFormat,
        ServiceConfig,
        TransportMode,
    };
    use starknet_transaction_prover::server::cors::{build_cors_layer, cors_mode};
    use starknet_transaction_prover::server::health::HealthLayer;
    use starknet_transaction_prover::server::metrics::{install_exporter, spawn_upkeep};
    use starknet_transaction_prover::server::panic::install_panic_hook;
    use starknet_transaction_prover::server::rpc_api::ProvingRpcServer;
    use starknet_transaction_prover::server::rpc_impl::ProvingRpcServerImpl;
    use starknet_transaction_prover::server::saturation::SaturationMonitor;
    use starknet_transaction_prover::server::shutdown::spawn_signal_bridge;
    use starknet_transaction_prover::server::{
        start_server,
        MetricsLayer,
        OhttpJsonrpseeLayer,
        ServerLayers,
        OHTTP_JSONRPSEE_BODY_BUILDER,
    };
    use tower_ohttp::OhttpGateway;
    use tracing::info;
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

    // Installed after tracing init so the hook's `error!` reaches the subscriber.
    // A panic before this point still goes to the default stderr handler.
    install_panic_hook();

    let config = ServiceConfig::from_args(args)?;

    config.log_startup_summary();

    // Install the Prometheus exporter and emit `prover_build_info` before binding, so a scrape
    // during a slow startup still returns the build identity.
    let prometheus_handle =
        install_exporter(env!("CARGO_PKG_VERSION"), option_env!("GIT_SHA").unwrap_or("unknown"))
            .context("Failed to install Prometheus exporter")?;
    let metrics_layer = MetricsLayer::new(prometheus_handle.clone());
    spawn_upkeep(prometheus_handle);

    // Build and start the JSON-RPC server. The request path and the health probe share one
    // saturation monitor. The request path records rejects and worker-slot progress, and the
    // probe reads it.
    let saturation_monitor = SaturationMonitor::default();
    let health_layer = HealthLayer::new(
        saturation_monitor.clone(),
        Duration::from_millis(config.health_max_saturated_ms),
    );
    let rpc_impl = ProvingRpcServerImpl::from_config(&config, saturation_monitor);
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
        ServerLayers { cors_layer, ohttp_layer, metrics_layer, health_layer },
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

    spawn_signal_bridge(server_handle.clone())?;

    server_handle.stopped().await;
    info!(event = "shutdown_complete", "JSON-RPC server stopped.");
    Ok(())
}
