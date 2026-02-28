//! TLS helpers for serving JSON-RPC over HTTPS.

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context};
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig as RustlsServerConfig;
use tokio_rustls::TlsAcceptor;

/// Loads a certificate chain and private key from PEM files and builds a TLS acceptor.
pub fn load_tls_acceptor(cert_path: &Path, key_path: &Path) -> anyhow::Result<TlsAcceptor> {
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
