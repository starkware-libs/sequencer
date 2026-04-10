//! OHTTP gateway: loads keys from environment and manages the
//! `ohttp::Server` instance for request decapsulation and response
//! encapsulation (RFC 9458).

use ohttp::hpke::{Aead, Kdf, Kem};
use ohttp::{KeyConfig, KeyId, Server, SymmetricSuite};
use tracing::info;

/// OHTTP gateway that holds the server key configuration and
/// decapsulates/encapsulates requests (RFC 9458).
pub struct OhttpGateway {
    server: Server,
    encoded_key_config: Vec<u8>,
}

/// Errors during OHTTP gateway initialization.
#[derive(Debug, thiserror::Error)]
pub enum OhttpGatewayError {
    #[error("OHTTP_KEY environment variable not set")]
    MissingEnvVar,
    #[error("OHTTP_KEY must be exactly 64 hex characters (32 bytes): {0}")]
    InvalidHex(String),
    #[error("failed to build OHTTP key config: {0}")]
    Config(#[source] ohttp::Error),
}

impl OhttpGateway {
    /// Load a single OHTTP key from the `OHTTP_KEY` environment variable.
    ///
    /// The value must be a hex-encoded 32-byte X25519 private key (64 hex chars).
    /// The public key is derived automatically and keyID is set to 0.
    pub fn from_env() -> Result<Self, OhttpGatewayError> {
        let hex_key = std::env::var("OHTTP_KEY").map_err(|_| OhttpGatewayError::MissingEnvVar)?;
        Self::from_hex_key(&hex_key)
    }

    /// Build from a hex-encoded X25519 private key string.
    pub fn from_hex_key(hex_key: &str) -> Result<Self, OhttpGatewayError> {
        let hex_key = hex_key.trim();
        let ikm = hex::decode(hex_key)
            .map_err(|_| OhttpGatewayError::InvalidHex(format!("invalid hex: {hex_key}")))?;
        if ikm.len() != 32 {
            return Err(OhttpGatewayError::InvalidHex(format!(
                "expected 32 bytes, got {}",
                ikm.len()
            )));
        }

        Self::from_ikm(0, &ikm)
    }

    /// Build from raw input keying material with a given key ID.
    fn from_ikm(key_id: KeyId, ikm: &[u8]) -> Result<Self, OhttpGatewayError> {
        let symmetric_suite = SymmetricSuite::new(Kdf::HkdfSha256, Aead::Aes128Gcm);
        let key_config = KeyConfig::derive(key_id, Kem::X25519Sha256, vec![symmetric_suite], ikm)
            .map_err(OhttpGatewayError::Config)?;

        let encoded_key_config =
            KeyConfig::encode_list(&[&key_config]).map_err(OhttpGatewayError::Config)?;

        let server = Server::new(key_config).map_err(OhttpGatewayError::Config)?;

        info!("OHTTP key loaded (keyID={key_id})");

        Ok(Self { server, encoded_key_config })
    }

    /// Returns the encoded key config bytes for the `GET /ohttp-keys` endpoint.
    /// Format: `application/ohttp-keys` (concatenated key configs with length prefixes).
    pub fn encoded_config(&self) -> &[u8] {
        &self.encoded_key_config
    }

    /// Returns a reference to the OHTTP server for decapsulating requests.
    pub fn server(&self) -> &Server {
        &self.server
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ikm() -> [u8; 32] {
        let mut ikm = [0u8; 32];
        ikm[0] = 1; // non-zero so key derivation produces a valid key
        ikm
    }

    #[test]
    fn from_ikm_produces_valid_config() {
        let gateway = OhttpGateway::from_ikm(0, &test_ikm()).unwrap();
        let config_bytes = gateway.encoded_config();
        assert!(!config_bytes.is_empty());
    }

    #[test]
    fn from_hex_key_roundtrip() {
        let hex = hex::encode(test_ikm());
        let gateway = OhttpGateway::from_hex_key(&hex).unwrap();
        assert!(!gateway.encoded_config().is_empty());
    }

    #[test]
    fn from_hex_key_rejects_short() {
        let result = OhttpGateway::from_hex_key("aabb");
        assert!(matches!(result, Err(OhttpGatewayError::InvalidHex(_))));
    }

    #[test]
    fn from_hex_key_rejects_invalid_hex() {
        let result = OhttpGateway::from_hex_key("zzzz");
        assert!(matches!(result, Err(OhttpGatewayError::InvalidHex(_))));
    }

    #[test]
    fn decapsulate_roundtrip() {
        let gateway = OhttpGateway::from_ikm(0, &test_ikm()).unwrap();

        // Client side: create a request using the server's key config.
        let config_bytes = gateway.encoded_config();
        let client_request = ohttp::ClientRequest::from_encoded_config_list(config_bytes).unwrap();

        let plaintext_request = b"test request body";
        let (encapsulated, client_response) =
            client_request.encapsulate(plaintext_request).unwrap();

        // Server side: decapsulate.
        let (decapsulated, server_response) = gateway.server().decapsulate(&encapsulated).unwrap();
        assert_eq!(decapsulated, plaintext_request);

        // Server side: encapsulate response.
        let plaintext_response = b"test response body";
        let encapsulated_response = server_response.encapsulate(plaintext_response).unwrap();

        // Client side: decapsulate response.
        let decapsulated_response = client_response.decapsulate(&encapsulated_response).unwrap();
        assert_eq!(decapsulated_response, plaintext_response);
    }
}
