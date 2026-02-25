//! CLI for Starknet OS Runner.

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;
use jsonrpsee::server::{ServerBuilder, ServerConfig};
use starknet_os_runner::metrics::{init_metrics, shutdown_metrics};
use starknet_os_runner::server::config::{CliArgs, ServiceConfig};
use starknet_os_runner::server::cors::{build_cors_layer, cors_mode};
use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
use tower::ServiceBuilder;
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO(Avi): Revisit the starknet_os_runner=debug default once the service stabilizes.
    // Initialize tracing with RUST_LOG (default: info,starknet_os_runner=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_os_runner=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Initialize OpenTelemetry metrics.
    let (metrics, meter_provider) = init_metrics(config.metrics_endpoint.as_deref());
    if config.metrics_endpoint.is_some() {
        info!(
            metrics_endpoint = config.metrics_endpoint.as_deref().unwrap_or_default(),
            "OTLP metrics export enabled"
        );
    } else {
        info!("Metrics export disabled (no --metrics-endpoint or OTEL_EXPORTER_OTLP_ENDPOINT)");
    }

    // Build and start the JSON-RPC server.
    let rpc_impl = ProvingRpcServerImpl::from_config(&config, metrics);
    let addr = SocketAddr::new(config.ip, config.port);

    let cors_layer = build_cors_layer(&config.cors_allow_origin)?;

    let server_config = ServerConfig::builder().max_connections(config.max_connections).build();
    let server = ServerBuilder::default()
        .set_config(server_config)
        .set_http_middleware(ServiceBuilder::new().option_layer(cors_layer))
        .build(&addr)
        .await
        .context(format!("Failed to bind JSON-RPC server to {addr}"))?;

    let handle = server.start(rpc_impl.into_rpc());
    info!(
        local_address = %addr,
        max_concurrent_requests = config.max_concurrent_requests,
        max_connections = config.max_connections,
        cors_mode = cors_mode(&config.cors_allow_origin),
        cors_allow_origin = ?config.cors_allow_origin,
        "JSON-RPC proving server is running."
    );

    handle.stopped().await;

    // Flush and shut down metrics on exit.
    shutdown_metrics(meter_provider);
    Ok(())
}
