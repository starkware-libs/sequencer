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

    use clap::Parser;
    use starknet_transaction_prover::proving::virtual_snos_prover::RpcVirtualSnosProver;
    use starknet_transaction_prover::server::config::{CliArgs, ServiceConfig, TransportMode};
    use starknet_transaction_prover::server::cors::{build_cors_layer, cors_mode};
    use starknet_transaction_prover::server::rpc_api::ProvingRpcServer;
    use starknet_transaction_prover::server::rpc_impl::ProvingRpcServerImpl;
    use starknet_transaction_prover::server::start_server;
    use tokio::sync::Semaphore;
    use tracing::info;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    // TODO(Avi): Revisit the starknet_transaction_prover=debug default once the service stabilizes.
    // Initialize tracing with RUST_LOG. By default, keep service logs and lower third-party
    // logs to warn.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,starknet_transaction_prover=debug,privacy_prove=info")
    });
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Build shared prover and concurrency semaphore (shared between JSON-RPC and binary
    // endpoints).
    let prover = RpcVirtualSnosProver::new(&config.prover_config);
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));

    let rpc_impl = ProvingRpcServerImpl::new(
        prover.clone(),
        config.max_concurrent_requests,
        semaphore.clone(),
    );
    let addr = SocketAddr::new(config.ip, config.port);
    let cors_layer = build_cors_layer(&config.cors_allow_origin)?;

    let scheme = match &config.transport {
        TransportMode::Http => "http",
        TransportMode::Https { .. } => "https",
    };

    let (local_addr, server_handle) = start_server(
        addr,
        &config.transport,
        rpc_impl.into_rpc().into(),
        config.max_connections,
        cors_layer,
        prover,
        config.max_concurrent_requests,
        semaphore,
    )
    .await?;

    info!(
        local_address = %local_addr,
        scheme,
        max_concurrent_requests = config.max_concurrent_requests,
        max_connections = config.max_connections,
        cors_mode = cors_mode(&config.cors_allow_origin),
        cors_allow_origin = ?config.cors_allow_origin,
        "JSON-RPC proving server is running."
    );

    server_handle.stopped().await;
    Ok(())
}
