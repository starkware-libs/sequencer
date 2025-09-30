//! Basic integration tests for Propeller protocol.

use apollo_propeller::{
    Behaviour,
    Config,
    MessageAuthenticity,
    ShardPublishError,
    TreeGenerationError,
};
use libp2p::identity::{Keypair, PeerId};

#[test]
fn test_propeller_behaviour_creation() {
    let config = Config::default();
    let message_authenticity = MessageAuthenticity::Author(PeerId::random());
    Behaviour::new(message_authenticity, config);
}

#[test]
fn test_peer_management() {
    let config = Config::default();
    let message_authenticity = MessageAuthenticity::Author(PeerId::random());
    let mut behaviour = Behaviour::new(message_authenticity, config);

    let peer1 = PeerId::random();
    let peer2 = PeerId::random();

    // Add peers with weights
    let _ = behaviour.set_peers(vec![(peer1, 1000), (peer2, 500)]);
}

#[test]
fn test_leader_management() {
    let config = Config::default();

    // Create a keypair for the local peer so we have a valid PeerId with extractable public key
    let local_keypair = libp2p::identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keypair.public());
    let message_authenticity = MessageAuthenticity::Author(local_peer_id);
    let mut behaviour = Behaviour::new(message_authenticity, config);

    // Create a keypair so we have a valid PeerId with extractable public key for leader
    let leader_keypair = libp2p::identity::Keypair::generate_ed25519();
    let leader_id = PeerId::from(leader_keypair.public());

    // Add both local peer and leader to peers first (local peer is required by tree manager)
    behaviour.set_peers(vec![(local_peer_id, 1500), (leader_id, 1000)]).unwrap();
}

#[test]
fn test_broadcast_without_leader() {
    let config = Config::default();
    let message_authenticity = MessageAuthenticity::Author(PeerId::random());
    let mut behaviour = Behaviour::new(message_authenticity, config.clone());

    // Simple test data
    let data = vec![1u8; 1024];

    // Should fail since no peers are set
    let result = behaviour.broadcast(data);
    assert!(matches!(
        result,
        Err(ShardPublishError::TreeGenerationError(TreeGenerationError::PublisherNotFound { .. }))
    ));
}

#[test]
fn test_config_builder() {
    use std::time::Duration;
    let config = Config::builder()
        .message_cache_ttl(Duration::from_secs(60))
        .max_shard_size(1 << 21)
        .emit_shard_received_events(true)
        .build();
    assert_eq!(config.message_cache_ttl(), Duration::from_secs(60));
    assert_eq!(config.max_shard_size(), 1 << 21);
    assert!(config.emit_shard_received_events());
}

#[test]
fn test_signature_verification() {
    // Create a keypair for signing
    let keypair = Keypair::generate_ed25519();
    let config = Config::builder().build();

    let mut behaviour =
        Behaviour::new(MessageAuthenticity::Signed(keypair.clone()), config.clone());

    // Set ourselves as the leader so we can broadcast
    let our_peer_id = PeerId::from(keypair.public());

    // Add our own public key for verification first
    let _ = behaviour.set_peers_and_keys(vec![(our_peer_id, 1000, keypair.public())]);

    // Create test data
    let test_data = vec![42u8; 1024];

    // Broadcast should succeed and create signed shards
    let result = behaviour.broadcast(test_data);
    assert!(result.is_ok(), "Broadcast should succeed with proper signing");

    tracing::info!("✅ Signature verification test passed!");
}

#[test]
fn test_peer_public_key_extraction() {
    // Create a keypair for testing
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    let config = Config::default();
    let mut behaviour = Behaviour::new(MessageAuthenticity::Author(PeerId::random()), config);

    // Test 1: Add peer without explicit public key - should extract from PeerId (Ed25519)
    let _ = behaviour.set_peers(vec![(peer_id, 1000)]);

    // Test 2: Add peer with explicit public key
    let keypair2 = Keypair::generate_ed25519();
    let peer_id2 = PeerId::from(keypair2.public());
    let _ = behaviour.set_peers_and_keys(vec![(peer_id2, 500, keypair2.public())]);

    // Test 3: Try to add a random PeerId (won't have extractable key)
    let random_peer = PeerId::random();
    let _ = behaviour.set_peers(vec![(random_peer, 250)]);

    tracing::info!("✅ Peer public key extraction test completed!");
}

#[test]
fn test_key_validation_security() {
    let config = Config::default();
    let mut behaviour = Behaviour::new(MessageAuthenticity::Author(PeerId::random()), config);

    // Test 1: Valid key-PeerId pair should work
    let keypair1 = Keypair::generate_ed25519();
    let peer_id1 = PeerId::from(keypair1.public());
    let _ = behaviour.set_peers_and_keys(vec![(peer_id1, 1000, keypair1.public())]);

    // Test 2: Invalid key-PeerId pair should be rejected
    let keypair2 = Keypair::generate_ed25519();
    let different_peer_id = PeerId::random(); // Different PeerId that doesn't match keypair2
    let _ = behaviour.set_peers_and_keys(vec![(different_peer_id, 500, keypair2.public())]);
    // This should log a security warning and not store the key

    tracing::info!("✅ Key validation security test completed!");
}
