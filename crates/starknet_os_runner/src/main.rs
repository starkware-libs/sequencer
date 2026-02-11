//! CLI for Starknet OS Runner.

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;
use http::{header, HeaderValue, Method};
use jsonrpsee::server::{ServerBuilder, ServerConfig};
use starknet_os_runner::server::config::{CliArgs, ServiceConfig};
use starknet_os_runner::server::rpc_impl::ProvingRpcServerImpl;
use starknet_os_runner::server::rpc_trait::ProvingRpcServer;
use tower::ServiceBuilder;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI args and load config.
    let args = CliArgs::parse();
    // Initialize tracing with RUST_LOG (default: info,starknet_os_runner=debug).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,starknet_os_runner=debug"));
    tracing_subscriber::registry().with(fmt::layer()).with(filter).init();
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

fn build_cors_layer(cors_allow_origin: &[String]) -> anyhow::Result<Option<CorsLayer>> {
    if cors_allow_origin.is_empty() {
        return Ok(None);
    }

    let allow_origin = if cors_allow_origin.iter().any(|origin| origin == "*") {
        AllowOrigin::any()
    } else if cors_allow_origin.len() == 1 {
        let header_value = HeaderValue::from_str(&cors_allow_origin[0]).context(format!(
            "Invalid cors_allow_origin header value '{}'",
            cors_allow_origin[0]
        ))?;
        AllowOrigin::exact(header_value)
    } else {
        let header_values = cors_allow_origin
            .iter()
            .map(|origin| {
                HeaderValue::from_str(origin)
                    .context(format!("Invalid cors_allow_origin header value '{origin}'"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        AllowOrigin::list(header_values)
    };

    Ok(Some(
        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods([Method::POST])
            .allow_headers([header::CONTENT_TYPE]),
    ))
}

fn cors_mode(cors_allow_origin: &[String]) -> &'static str {
    if cors_allow_origin.is_empty() {
        "disabled"
    } else if cors_allow_origin.len() == 1 && cors_allow_origin[0] == "*" {
        "wildcard"
    } else {
        "allowlist"
    }
}
