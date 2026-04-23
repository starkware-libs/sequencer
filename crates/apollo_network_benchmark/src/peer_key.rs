use anyhow::Context;
use libp2p::identity::Keypair;

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

#[cfg(test)]
mod tests {
    use super::{peer_id_from_node_id, private_key_from_node_id};

    #[test]
    fn private_key_layout_is_node_id_le_then_zeros() {
        // Catches accidental BE/LE flips or seed-layout changes that would silently
        // re-key every benchmark identity.
        let mut expected = [0u8; 32];
        expected[..8].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(private_key_from_node_id(1), expected);

        let mut expected_two_fifty_six = [0u8; 32];
        expected_two_fifty_six[..8].copy_from_slice(&256u64.to_le_bytes());
        assert_eq!(private_key_from_node_id(256), expected_two_fifty_six);
    }

    #[test]
    fn peer_id_derivation_is_deterministic_and_distinct_per_node_id() {
        let peer_id_zero =
            peer_id_from_node_id(0).expect("derivation should succeed for node_id 0");
        let peer_id_zero_again =
            peer_id_from_node_id(0).expect("derivation should succeed for node_id 0");
        let peer_id_one = peer_id_from_node_id(1).expect("derivation should succeed for node_id 1");

        assert_eq!(peer_id_zero, peer_id_zero_again);
        assert_ne!(peer_id_zero, peer_id_one);
    }
}
