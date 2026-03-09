use libp2p::identity::Keypair;
use libp2p::PeerId;
use rstest::rstest;

use crate::signature::{
    sign_message_id,
    try_extract_public_key_from_peer_id,
    validate_public_key_matches_peer_id,
    verify_message_id_signature,
};
use crate::types::{Channel, MessageRoot};

const TEST_CHANNEL: Channel = Channel(1);
const TEST_NONCE: u64 = 1_700_000_000_000_000_000;

#[rstest]
#[case::matching_params(MessageRoot([1; 32]), TEST_CHANNEL, TEST_NONCE, true)]
#[case::wrong_root(MessageRoot([2; 32]), TEST_CHANNEL, TEST_NONCE, false)]
#[case::wrong_channel(MessageRoot([1; 32]), Channel(99), TEST_NONCE, false)]
#[case::wrong_nonce(MessageRoot([1; 32]), TEST_CHANNEL, TEST_NONCE + 1, false)]
fn test_sign_and_verify(
    #[case] verify_root: MessageRoot,
    #[case] verify_channel: Channel,
    #[case] verify_nonce: u64,
    #[case] expect_valid: bool,
) {
    let keypair = Keypair::generate_ed25519();
    let sign_root = MessageRoot([1; 32]);

    let signature = sign_message_id(&sign_root, TEST_CHANNEL, TEST_NONCE, &keypair).unwrap();
    assert!(!signature.is_empty());

    let result = verify_message_id_signature(
        &verify_root,
        verify_channel,
        verify_nonce,
        &signature,
        &keypair.public(),
    );
    assert_eq!(result.is_ok(), expect_valid);
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
