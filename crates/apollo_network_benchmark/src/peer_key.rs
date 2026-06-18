use anyhow::Context;
use libp2p::identity::Keypair;

#[cfg(test)]
#[path = "peer_key_test.rs"]
mod peer_key_test;

/// Derives a deterministic ed25519 private key from a node index.
///
/// FOR BENCHMARK USE ONLY. The seed layout (8 LE bytes of `node_id` followed by 24 zero
/// bytes) is intentionally trivial so peers can derive each other's PeerIds from their
/// `node_id`s alone. This produces predictable keys and must never be used outside the
/// benchmark binary.
///
/// The byte layout is load-bearing across processes: changing it re-keys every benchmark
/// identity and breaks bootstrap multiaddr lists. Regression-tested below.
pub fn private_key_from_node_id(node_id: u64) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&node_id.to_le_bytes());
    key
}

/// Returns the PeerId string for a given node index. See [`private_key_from_node_id`] —
/// this is benchmark-only and must not be used as a real peer identity.
pub fn peer_id_from_node_id(node_id: u64) -> anyhow::Result<String> {
    let secret_key_bytes = private_key_from_node_id(node_id);
    let keypair = Keypair::ed25519_from_bytes(secret_key_bytes)
        .context("Failed to derive keypair from node id")?;
    Ok(keypair.public().to_peer_id().to_string())
}
