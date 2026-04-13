//! TLS helpers for serving JSON-RPC over HTTPS.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context};
use jsonrpsee::server::{
    serve_with_graceful_shutdown,
    stop_channel,
    HttpBody,
    Methods,
    ServerBuilder,
    ServerConfig,
    ServerHandle,
};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig as RustlsServerConfig;
use tokio_rustls::TlsAcceptor;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::map_request_body::MapRequestBodyLayer;
use tower_http::map_response_body::MapResponseBodyLayer;
use tracing::warn;

use super::OhttpJsonrpseeLayer;

/// Maximum time allowed for a TLS handshake before the connection is dropped.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Binds an HTTPS JSON-RPC server using the given TLS certificate and key.
///
/// Returns the bound local address and a handle that can be used to await or stop the server.
#[allow(clippy::too_many_arguments)]
pub async fn start_tls_server(
    addr: SocketAddr,
    cert_path: &Path,
    key_path: &Path,
    methods: impl Into<Methods>,
    max_connections: u32,
    max_request_body_size: u32,
    cors_layer: Option<CorsLayer>,
    ohttp_layer: Option<OhttpJsonrpseeLayer>,
) -> anyhow::Result<(SocketAddr, ServerHandle)> {
    let tls_acceptor = load_tls_acceptor(cert_path, key_path)?;

    let server_config = ServerConfig::builder()
        .max_connections(max_connections)
        .max_request_body_size(max_request_body_size)
        .build();
    // See `server.rs` for the rationale — `OhttpLayer` sits outside `CompressionLayer`
    // so compression acts on the inner JSON-RPC response, not on the OHTTP envelope.
    // `MapRequestBodyLayer`/`MapResponseBodyLayer` keep `HttpBody` on both sides of
    // OHTTP to satisfy its symmetric-body bound.
    let svc_builder = ServerBuilder::default()
        .set_config(server_config)
        .set_http_middleware(
            ServiceBuilder::new()
                .option_layer(cors_layer)
                .layer(MapRequestBodyLayer::new(HttpBody::new))
                .option_layer(ohttp_layer)
                .layer(MapResponseBodyLayer::new(HttpBody::new))
                .layer(CompressionLayer::new()),
        )
        .to_service_builder();

    let listener = TcpListener::bind(addr)
        .await
        .context(format!("Failed to bind HTTPS JSON-RPC server to {addr}"))?;
    let local_addr =
        listener.local_addr().context("Failed to read local address for HTTPS listener")?;

    let methods: Methods = methods.into();
    let (stop_handle, server_handle) = stop_channel();

    tokio::spawn(async move {
        loop {
            let accept_result = tokio::select! {
                accept_result = listener.accept() => accept_result,
                _ = stop_handle.clone().shutdown() => break,
            };

            let (socket, remote_addr) = match accept_result {
                Ok(conn) => conn,
                Err(err) => {
                    warn!(error = %err, "Failed to accept incoming TCP connection");
                    continue;
                }
            };

            let tls_acceptor = tls_acceptor.clone();
            let stop_handle = stop_handle.clone();
            let methods = methods.clone();
            let svc_builder = svc_builder.clone();

            tokio::spawn(async move {
                let tls_stream =
                    match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, tls_acceptor.accept(socket))
                        .await
                    {
                        Ok(Ok(stream)) => stream,
                        Ok(Err(err)) => {
                            warn!(
                                remote_address = %remote_addr,
                                error = %err,
                                "TLS handshake failed"
                            );
                            return;
                        }
                        Err(_) => {
                            warn!(
                                remote_address = %remote_addr,
                                "TLS handshake timed out"
                            );
                            return;
                        }
                    };

                let svc = svc_builder.build(methods, stop_handle.clone());
                if let Err(err) =
                    serve_with_graceful_shutdown(tls_stream, svc, stop_handle.shutdown()).await
                {
                    warn!(
                        remote_address = %remote_addr,
                        error = %err,
                        "HTTPS connection terminated with error"
                    );
                }
            });
        }
    });

    Ok((local_addr, server_handle))
}

/// Loads a certificate chain and private key from PEM files and builds a TLS acceptor.
fn load_tls_acceptor(cert_path: &Path, key_path: &Path) -> anyhow::Result<TlsAcceptor> {
    let cert_pem = std::fs::read(cert_path)
        .with_context(|| format!("Failed to read TLS certificate file: {}", cert_path.display()))?;
    let cert_chain: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(&cert_pem).collect::<Result<Vec<_>, _>>().with_context(
            || format!("Failed to parse TLS certificate PEM chain from {}", cert_path.display()),
        )?;
    if cert_chain.is_empty() {
        bail!(
            "TLS certificate file does not contain any certificate PEM blocks: {}",
            cert_path.display()
        );
    }

    let key_pem = std::fs::read(key_path)
        .with_context(|| format!("Failed to read TLS private key file: {}", key_path.display()))?;
    let private_key = PrivateKeyDer::from_pem_slice(&key_pem).with_context(|| {
        format!(
            "Failed to parse TLS private key PEM from {} (supported: PKCS#1, PKCS#8, SEC1)",
            key_path.display()
        )
    })?;

    let mut tls_config = RustlsServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)
        .context("Failed to construct TLS server configuration from certificate and key")?;
    tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(Arc::new(tls_config)))
}
