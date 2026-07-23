//! OHTTP gateway: loads HPKE keys from the environment or from raw material
//! and exposes the `ohttp::Server` for request decapsulation (RFC 9458).

use ohttp::hpke::{Aead, Kdf, Kem};
use ohttp::{KeyConfig, KeyId, Server, SymmetricSuite};
use tracing::info;

use crate::errors::OhttpError;

#[cfg(test)]
#[path = "gateway_test.rs"]
mod gateway_test;

/// Default KEM used by `from_env` / `from_hex_key`. Currently X25519 — the
/// only variant `ohttp 0.7` supports. When newer versions add P-256 etc.,
/// add sibling constructors (`from_pem_p256`, etc.) that call `from_ikm`
/// with the appropriate `Kem` value.
const DEFAULT_KEM: Kem = Kem::X25519Sha256;

/// Default HPKE symmetric suite. Kept hardcoded because the combinations
/// that make cryptographic sense are narrow and these are secure defaults.
fn default_suite() -> SymmetricSuite {
    SymmetricSuite::new(Kdf::HkdfSha256, Aead::Aes128Gcm)
}

/// Expected raw key length for a given KEM.
fn kem_ikm_len(kem: Kem) -> usize {
    match kem {
        Kem::X25519Sha256 => 32,
    }
}

/// OHTTP gateway that holds the server key configuration and
/// decapsulates/encapsulates requests (RFC 9458).
pub struct OhttpGateway {
    server: Server,
    encoded_key_config: Vec<u8>,
}

impl OhttpGateway {
    /// Load a single X25519 OHTTP key from the `OHTTP_KEY` environment
    /// variable. The value must be a hex-encoded 32-byte private key
    /// (64 hex chars). Key ID is 0.
    pub fn from_env() -> Result<Self, OhttpError> {
        let hex_key = std::env::var("OHTTP_KEY").map_err(|_| OhttpError::MissingKeyEnvVar)?;
        Self::from_hex_key(&hex_key)
    }

    /// Build from a hex-encoded X25519 private key string (64 hex chars).
    /// Key ID is 0.
    pub fn from_hex_key(hex_key: &str) -> Result<Self, OhttpError> {
        let hex_key = hex_key.trim();
        let ikm = hex::decode(hex_key).map_err(|_| {
            OhttpError::InvalidKey("hex decode failed (expected 64 hex characters)".to_string())
        })?;
        let expected = kem_ikm_len(DEFAULT_KEM);
        if ikm.len() != expected {
            return Err(OhttpError::InvalidKey(format!(
                "expected {expected} bytes, got {}",
                ikm.len()
            )));
        }
        Self::from_ikm(0, DEFAULT_KEM, &ikm)
    }

    /// Low-level constructor: build a gateway from raw key material and an
    /// explicit KEM. This is the path future constructors (e.g. PEM-loaded
    /// P-256 keys) route through.
    pub fn from_ikm(key_id: KeyId, kem: Kem, ikm: &[u8]) -> Result<Self, OhttpError> {
        let key_config = KeyConfig::derive(key_id, kem, vec![default_suite()], ikm)
            .map_err(OhttpError::KeyConfig)?;

        let encoded_key_config =
            KeyConfig::encode_list(&[&key_config]).map_err(OhttpError::KeyConfig)?;

        let server = Server::new(key_config).map_err(OhttpError::KeyConfig)?;

        info!(kem = ?kem, key_id, "OHTTP key loaded");

        Ok(Self { server, encoded_key_config })
    }

    /// Returns the encoded key config bytes for the `GET /ohttp-keys` endpoint.
    /// Format: `application/ohttp-keys` (concatenated key configs with length prefixes).
    pub fn encoded_config(&self) -> &[u8] {
        &self.encoded_key_config
    }

    /// Returns a reference to the underlying OHTTP server for decapsulation.
    pub fn server(&self) -> &Server {
        &self.server
    }
}
