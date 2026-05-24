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
use tower_http::map_response_body::MapResponseBodyLayer;
use tower_ohttp::OhttpLayer;

use self::config::TransportMode;

/// `OhttpLayer` specialized for jsonrpsee's response body type. The
/// `fn(...)` body builder is `Fn + Send + Sync + 'static` automatically.
pub type OhttpJsonrpseeLayer = OhttpLayer<fn(Full<Bytes>) -> HttpBody>;

/// The body builder used by the jsonrpsee-specialized OHTTP layer.
/// Pass this to `OhttpLayer::new` when constructing the layer.
pub const OHTTP_JSONRPSEE_BODY_BUILDER: fn(Full<Bytes>) -> HttpBody = HttpBody::new;

/// The production HTTP middleware stack, applied verbatim by both transports:
/// the HTTP path (`start_server`) and the HTTPS path (`tls::start_tls_server`).
///
/// This is a macro rather than a helper function because the value is a deeply
/// nested `ServiceBuilder<Stack<Stack<...>>>` whose type cannot be spelled in a
/// signature. Defining the chain once keeps the two transports from drifting.
///
/// Defined above `pub mod tls;` on purpose: `macro_rules!` scoping is textual, so moving the
/// definition below the module would make it invisible to `tls.rs`. The macro also resolves
/// `ServiceBuilder`, the layer types, and `HttpBody` at each call site (not at the definition),
/// so every caller must have them in scope — adding a layer here adds an import obligation at
/// each call site.
///
/// Layer order (tower makes the last-added layer innermost):
/// - `RequestLogLayer` is outermost so the latency it measures covers every other layer.
/// - `HealthLayer` sits inside it so probes complete before CORS/OHTTP.
/// - `OhttpLayer` must sit OUTSIDE `CompressionLayer` so compression applies to the inner JSON-RPC
///   response (the client's inner `Accept-Encoding` travels through BHTTP into jsonrpsee) rather
///   than to the OHTTP ciphertext envelope. `MapRequestBodyLayer`/`MapResponseBodyLayer` keep
///   `HttpBody` on both sides of OHTTP to satisfy its symmetric-body bound; `HttpBody::new` is a
///   zero-cost wrapper, so non-OHTTP requests still stream through unbuffered.
/// - `RequestSpanLayer` sits BELOW `OhttpLayer` so it spans the decapsulated inner request with a
///   fresh, envelope-unlinkable id (see `request_span`).
macro_rules! prover_http_middleware {
    ($cors_layer:expr, $ohttp_layer:expr $(,)?) => {
        ServiceBuilder::new()
            .layer(RequestLogLayer)
            .layer(HealthLayer)
            .option_layer($cors_layer)
            .layer(MapRequestBodyLayer::new(HttpBody::new))
            .option_layer($ohttp_layer)
            .layer(RequestSpanLayer)
            .layer(MapResponseBodyLayer::new(HttpBody::new))
            .layer(CompressionLayer::new())
    };
}

pub mod config;
pub mod cors;
pub mod errors;
pub mod health;
pub mod log_redact;
#[cfg(test)]
pub mod mock_rpc;
pub mod panic;
pub mod request_log;
pub mod request_span;
pub mod rpc_api;
pub mod rpc_impl;
pub mod tls;

pub use health::{HealthLayer, HEALTH_PATH};
pub use request_log::{RequestLogLayer, REQUEST_ID_HEADER};
pub use request_span::RequestSpanLayer;

#[cfg(test)]
mod rpc_spec_test;

#[cfg(test)]
mod request_body_size_test;

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
                // See `prover_http_middleware!` for the full layer-order rationale.
                .set_http_middleware(prover_http_middleware!(cors_layer, ohttp_layer))
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
