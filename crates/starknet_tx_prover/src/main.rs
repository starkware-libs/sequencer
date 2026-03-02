//! CLI for the Starknet transaction prover.

#[cfg(not(feature = "stwo_proving"))]
fn main() {
    eprintln!("The `starknet_tx_prover` binary requires the `stwo_proving` feature.");
    std::process::exit(1);
}

#[cfg(feature = "stwo_proving")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;

    use anyhow::Context;
    use clap::Parser;
    use jsonrpsee::server::{ServerBuilder, ServerConfig};
    use starknet_tx_prover::server::config::{CliArgs, ServiceConfig};
    use starknet_tx_prover::server::cors::{build_cors_layer, cors_mode};
    use starknet_tx_prover::server::rpc_api::ProvingRpcServer;
    use starknet_tx_prover::server::rpc_impl::ProvingRpcServerImpl;
    use tower::ServiceBuilder;
    use tracing::info;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    // TODO(Avi): Revisit the starknet_tx_prover=debug default once the service stabilizes.
    // Initialize tracing with RUST_LOG (default: info,starknet_tx_prover=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_tx_prover=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Build and start the JSON-RPC server.
    let rpc_impl = ProvingRpcServerImpl::from_config(&config);
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
    Ok(())
}
