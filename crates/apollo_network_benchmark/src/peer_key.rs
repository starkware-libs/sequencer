use anyhow::Context;
use libp2p::identity::Keypair;

/// Derives a deterministic ed25519 private key from a node index.
/// The index is written as little-endian bytes into the first 8 bytes of a 32-byte seed.
pub fn private_key_from_node_id(node_id: u64) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&node_id.to_le_bytes());
    key
}

/// Returns the PeerId string for a given node index.
pub fn peer_id_from_node_id(node_id: u64) -> anyhow::Result<String> {
    let secret_key_bytes = private_key_from_node_id(node_id);
    let keypair = Keypair::ed25519_from_bytes(secret_key_bytes)
        .context("Failed to derive keypair from node id")?;
    Ok(keypair.public().to_peer_id().to_string())
}
