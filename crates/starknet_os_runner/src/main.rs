//! CLI for Starknet OS Runner.

use clap::Parser;
use starknet_os_runner::server::http_server::{CliArgs, ProvingHttpServer, ServiceConfig};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with RUST_LOG (default: info,starknet_os_runner=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_os_runner=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();

    // Parse CLI args and load config.
    let args = CliArgs::parse();
    let config = ServiceConfig::from_args(args)?;

    // Create and run server.
    let server = ProvingHttpServer::new(config);
    server.run().await?;

    Ok(())
}
