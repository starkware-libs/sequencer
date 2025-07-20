// Derived using `get_peer_id_from_secret_key` binary, where the secret key of node with index `id`
// is format!("0x010101010101010101010101010101010101010101010101010101010101010{}", id + 1)

const PEER_IDS: [&str; 6] = [
    "12D3KooWK99VoVxNE7XzyBwXEzW7xhK7Gpv85r9F3V3fyKSUKPH5",
    "12D3KooWCPzcTZ4ymgyveYaFfZ4bfWsBEh2KxuxM3Rmy7MunqHwe",
    "12D3KooWT3eoCYeMPrSNnF1eQHimWFDiqPkna7FUD6XKBw8oPiMp",
    "12D3KooWFdTjV6DXVJfQFisTXadCsqGzCbEnJJWzc6mXSPwy9g54",
    "12D3KooWJMukrrip9sUyto28eiofqxyXiw9sfTJuZeQfHUujWPX8",
    "12D3KooWMqkzSDGNQg9WDDPdu7nQgAPpqTY3YqZ2XUYqJzmUhmVu",
];

<<<<<<< HEAD
// TODO(Tsabary): Get rid of the secret key type and its usage; there should be a precomputed map
// from node index to its peer id.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretKey([u8; 32]);

impl TryFrom<&str> for SecretKey {
    type Error = String;

    fn try_from(hex_str: &str) -> Result<Self, Self::Error> {
        let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
            .map_err(|e| format!("Invalid hex: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!("Expected 32 bytes, got {}", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(SecretKey(arr))
    }
}

impl TryFrom<String> for SecretKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl AsRef<[u8]> for SecretKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for SecretKey {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

pub(crate) fn get_peer_id(secret_key: SecretKey) -> String {
    Keypair::ed25519_from_bytes(secret_key).unwrap().public().to_peer_id().to_string()
||||||| 199fa631c
// TODO(Tsabary): Get rid of the secret key type and its usage; there should be a precomputed map
// from node index to its peer id.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretKey([u8; 32]);

impl TryFrom<&str> for SecretKey {
    type Error = String;

    fn try_from(hex_str: &str) -> Result<Self, Self::Error> {
        let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(hex_str))
            .map_err(|e| format!("Invalid hex: {}", e))?;
        if bytes.len() != 32 {
            return Err(format!("Expected 32 bytes, got {}", bytes.len()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(SecretKey(arr))
    }
}

impl TryFrom<String> for SecretKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl AsRef<[u8]> for SecretKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for SecretKey {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

pub(crate) fn get_peer_id(secret_key: SecretKey) -> String {
    Keypair::ed25519_from_bytes(secret_key).unwrap().public().to_peer_id().to_string()
=======
pub(crate) fn get_peer_id(node_id: usize) -> String {
    assert!(node_id < PEER_IDS.len(), "Node index out of bounds: {}", node_id);
    PEER_IDS[node_id].to_string()
>>>>>>> origin/main-v0.14.0
}

pub(crate) fn get_p2p_address(dns: &str, port: u16, peer_id: &str) -> String {
    format!("/dns/{dns}/udp/{port}/quic-v1/p2p/{peer_id}")
}
