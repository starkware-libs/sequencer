//! CLI for Starknet OS Runner.

#[cfg(not(feature = "stwo_proving"))]
fn main() {
    eprintln!("The `starknet_os_runner` binary requires the `stwo_proving` feature.");
    std::process::exit(1);
}

#[cfg(feature = "stwo_proving")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use std::net::SocketAddr;

    use clap::Parser;
    use starknet_os_runner::server::config::{CliArgs, ServiceConfig, TransportMode};
    use starknet_os_runner::server::cors::{build_cors_layer, cors_mode};
    use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
    use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
    use starknet_os_runner::server::start_server;
    use tracing::info;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    // TODO(Avi): Revisit the starknet_os_runner=debug default once the service stabilizes.
    // Initialize tracing with RUST_LOG (default: info,starknet_os_runner=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_os_runner=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Build and start the JSON-RPC server.
    let rpc_impl = ProvingRpcServerImpl::from_config(&config);
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
