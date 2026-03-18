//! JSON-RPC server exposing the proving pipeline.
//!
//! Provides the HTTP entry point, concurrency limiting, CORS configuration, and error mapping
//! from internal prover errors to JSON-RPC error codes.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use jsonrpsee::server::{Methods, ServerBuilder, ServerConfig, ServerHandle};
use tokio::sync::Semaphore;
use tower::ServiceBuilder;
use tower_http::cors::CorsLayer;

use self::binary_proof::BinaryProofLayer;
use self::config::TransportMode;
use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;

pub mod binary_proof;
pub mod config;
pub mod cors;
pub mod errors;
#[cfg(test)]
pub mod mock_rpc;
pub mod rpc_api;
pub mod rpc_impl;
pub mod tls;

#[cfg(test)]
mod rpc_spec_test;

/// Starts the JSON-RPC server in either HTTP or HTTPS mode depending on the transport.
///
/// The server exposes both the JSON-RPC API and a binary proof endpoint at `POST /proof`
/// that returns raw proof bytes as `application/octet-stream`.
pub async fn start_server(
    addr: SocketAddr,
    transport: &TransportMode,
    methods: Methods,
    max_connections: u32,
    cors_layer: Option<CorsLayer>,
    prover: RpcVirtualSnosProver,
    max_concurrent_requests: usize,
    semaphore: Arc<Semaphore>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    let binary_proof_layer = BinaryProofLayer::new(prover, max_concurrent_requests, semaphore);

    match transport {
        TransportMode::Http => {
            let server_config = ServerConfig::builder().max_connections(max_connections).build();
            let server = ServerBuilder::default()
                .set_config(server_config)
                .set_http_middleware(
                    ServiceBuilder::new().layer(binary_proof_layer).option_layer(cors_layer),
                )
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
                binary_proof_layer,
            )
            .await
        }
    }
}
