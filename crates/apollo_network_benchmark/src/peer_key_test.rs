use super::{peer_id_from_node_id, private_key_from_node_id};

#[test]
fn private_key_layout_is_node_id_le_then_zeros() {
    // Catches accidental BE/LE flips or seed-layout changes that would silently
    // re-key every benchmark identity.
    let key_one = private_key_from_node_id(1);
    let mut expected_one = [0u8; 32];
    expected_one[..8].copy_from_slice(&1u64.to_le_bytes());
    assert_eq!(key_one, expected_one);
    // The "then zeros" half of the contract: bytes 8..32 must remain zero.
    assert_eq!(&key_one[8..], &[0u8; 24]);

    // 256 spans the second byte, so this confirms LE encoding (not just byte 0).
    let key_two_fifty_six = private_key_from_node_id(256);
    let mut expected_two_fifty_six = [0u8; 32];
    expected_two_fifty_six[..8].copy_from_slice(&256u64.to_le_bytes());
    assert_eq!(key_two_fifty_six, expected_two_fifty_six);
    assert_eq!(&key_two_fifty_six[8..], &[0u8; 24]);
}

#[test]
fn peer_id_derivation_is_deterministic_and_distinct_per_node_id() {
    let peer_id_zero = peer_id_from_node_id(0).expect("derivation should succeed for node_id 0");
    let peer_id_zero_again =
        peer_id_from_node_id(0).expect("derivation should succeed for node_id 0");
    let peer_id_one = peer_id_from_node_id(1).expect("derivation should succeed for node_id 1");

    assert_eq!(peer_id_zero, peer_id_zero_again);
    assert_ne!(peer_id_zero, peer_id_one);
}
