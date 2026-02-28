use std::net::SocketAddr;

use anyhow::Context;
use jsonrpsee::server::{Methods, ServerBuilder, ServerConfig, ServerHandle};
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use self::config::TransportMode;

pub mod config;
pub mod cors;
pub mod error;
pub mod rpc_impl;
pub mod rpc_trait;
pub mod tls;

/// Starts the JSON-RPC server in either HTTP or HTTPS mode depending on the transport.
pub async fn start_server(
    addr: SocketAddr,
    transport: &TransportMode,
    methods: Methods,
    max_connections: u32,
    cors_layer: Option<CorsLayer>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    match transport {
        TransportMode::Http => {
            let server_config =
                ServerConfig::builder().max_connections(max_connections).build();
            let server = ServerBuilder::default()
                .set_config(server_config)
                .set_http_middleware(ServiceBuilder::new().option_layer(cors_layer))
                .build(&addr)
                .await
                .context(format!("Failed to bind JSON-RPC server to {addr}"))?;
            let local_addr = server.local_addr()?;
            let server_handle = server.start(methods);
            Ok((local_addr, server_handle))
        }
        TransportMode::Https { tls_cert_file, tls_key_file } => {
            tls::start_tls_server(
                addr,
                tls_cert_file,
                tls_key_file,
                methods,
                max_connections,
                cors_layer,
            )
            .await
        }
    }
}
