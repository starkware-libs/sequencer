use libp2p::identity::Keypair;
use libp2p::PeerId;

use crate::signature::{
    sign_message_id,
    try_extract_public_key_from_peer_id,
    validate_public_key_matches_peer_id,
    verify_message_id_signature,
};
use crate::types::MessageRoot;

#[test]
fn test_sign_and_verify_merkle_root() {
    let keypair = Keypair::generate_ed25519();

    let merkle_root = MessageRoot([1; 32]);

    // Sign the merkle root
    let signature = sign_message_id(&merkle_root, &keypair).unwrap();
    assert!(!signature.is_empty());

    // Verify the signature
    let result = verify_message_id_signature(&merkle_root, &signature, &keypair.public());
    assert!(result.is_ok());
}

#[test]
fn test_sign_and_verify_fails_with_wrong_data() {
    let keypair = Keypair::generate_ed25519();

    let merkle_root = MessageRoot([1u8; 32]);
    let different_root = MessageRoot([2u8; 32]);

    // Sign the merkle root
    let signature = sign_message_id(&merkle_root, &keypair).unwrap();

    // Verify with different data should fail
    let result = verify_message_id_signature(&different_root, &signature, &keypair.public());
    assert!(result.is_err());
}

#[test]
fn test_key_extraction_and_validation() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    // Test extraction
    let extracted_key = try_extract_public_key_from_peer_id(&peer_id);
    assert!(extracted_key.is_some());

    // Test validation
    let is_valid = validate_public_key_matches_peer_id(&keypair.public(), &peer_id);
    assert!(is_valid);

    // Test with mismatched key
    let other_keypair = Keypair::generate_ed25519();
    let is_invalid = validate_public_key_matches_peer_id(&other_keypair.public(), &peer_id);
    assert!(!is_invalid);
}

#[test]
fn test_random_peer_id_extraction() {
    let random_peer = PeerId::random();
    let extracted_key = try_extract_public_key_from_peer_id(&random_peer);
    assert!(extracted_key.is_none()); // Should fail for random PeerIDs
}
