//! CLI for Starknet OS Runner.

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;
use jsonrpsee::server::{ServerBuilder, ServerConfig};
use starknet_os_runner::server::config::{CliArgs, ServiceConfig};
use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let server_config = ServerConfig::builder().max_connections(config.max_connections).build();
    let server = ServerBuilder::default()
        .set_config(server_config)
        .build(&addr)
        .await
        .context(format!("Failed to bind JSON-RPC server to {addr}"))?;

    let handle = server.start(rpc_impl.into_rpc());
    info!(
        local_address = %addr,
        max_concurrent_requests = config.max_concurrent_requests,
        max_connections = config.max_connections,
        "JSON-RPC proving server is running."
    );

    handle.stopped().await;
    Ok(())
}
