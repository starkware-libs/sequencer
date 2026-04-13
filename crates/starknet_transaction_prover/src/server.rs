//! JSON-RPC server exposing the proving pipeline.
//!
//! Provides the HTTP entry point, concurrency limiting, CORS configuration, and error mapping
//! from internal prover errors to JSON-RPC error codes.

use std::net::SocketAddr;

use anyhow::Context;
use bytes::Bytes;
use http_body_util::Full;
use jsonrpsee::server::{HttpBody, Methods, ServerBuilder, ServerConfig, ServerHandle};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::map_request_body::MapRequestBodyLayer;
use tower_ohttp::OhttpLayer;

use self::config::TransportMode;

/// `OhttpLayer` specialized for jsonrpsee's response body type. The
/// `fn(...)` body builder is `Fn + Send + Sync + 'static` automatically.
pub type OhttpJsonrpseeLayer = OhttpLayer<fn(Full<Bytes>) -> HttpBody>;

/// The body builder used by the jsonrpsee-specialized OHTTP layer.
/// Pass this to `OhttpLayer::new` when constructing the layer.
pub const OHTTP_JSONRPSEE_BODY_BUILDER: fn(Full<Bytes>) -> HttpBody = HttpBody::new;

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

#[cfg(test)]
#[path = "server/ohttp_integration_test.rs"]
mod ohttp_integration_test;

/// Starts the JSON-RPC server in either HTTP or HTTPS mode depending on the transport.
pub async fn start_server(
    addr: SocketAddr,
    transport: &TransportMode,
    methods: Methods,
    max_connections: u32,
    max_request_body_size: u32,
    cors_layer: Option<CorsLayer>,
    ohttp_layer: Option<OhttpJsonrpseeLayer>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    match transport {
        TransportMode::Http => {
            let server_config = ServerConfig::builder()
                .max_connections(max_connections)
                .max_request_body_size(max_request_body_size)
                .build();
            let server = ServerBuilder::default()
                .set_config(server_config)
                // `MapRequestBodyLayer` wraps hyper's `Request<Incoming>` into
                // `Request<HttpBody>` before `OhttpLayer` sees it. jsonrpsee's
                // `TowerServiceNoHttp` always returns `Response<HttpBody>` regardless of
                // the incoming body type, so without this wrap the ohttp layer's
                // symmetric-body bound can't be satisfied. `HttpBody::new` is a zero-cost
                // wrapper (no buffering), so non-OHTTP requests still stream through.
                .set_http_middleware(
                    ServiceBuilder::new()
                        .option_layer(cors_layer)
                        .layer(CompressionLayer::new())
                        .layer(MapRequestBodyLayer::new(HttpBody::new))
                        .option_layer(ohttp_layer),
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
                max_request_body_size,
                cors_layer,
                ohttp_layer,
            )
            .await
        }
    }
}
