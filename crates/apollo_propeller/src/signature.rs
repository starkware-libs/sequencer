//! Signature creation and validation for the Propeller protocol.
//!
//! This module handles cryptographic operations for signing and verifying messages,
//! following similar patterns to gossipsub for consistency with the libp2p ecosystem.

use libp2p::identity::{Keypair, PeerId, PublicKey};
use tracing::{debug, warn};

use crate::types::{MessageRoot, ShardPublishError, ShardSignatureVerificationError};

pub(crate) const SIGNING_PREFIX: &[u8] = b"libp2p-propeller:";

pub(crate) fn sign_message_id(
    message_id: &MessageRoot,
    keypair: &Keypair,
) -> Result<Vec<u8>, ShardPublishError> {
    let msg = [SIGNING_PREFIX, &message_id.0].concat();
    match keypair.sign(&msg) {
        Ok(signature) => Ok(signature),
        Err(e) => Err(ShardPublishError::SigningFailed(e.to_string())),
    }
}

pub(crate) fn verify_message_id_signature(
    message_id: &MessageRoot,
    signature: &[u8],
    public_key: &PublicKey,
) -> Result<(), ShardSignatureVerificationError> {
    if signature.is_empty() {
        return Err(ShardSignatureVerificationError::EmptySignature);
    }
    let msg = [SIGNING_PREFIX, &message_id.0].concat();
    let signature_valid = public_key.verify(&msg, signature);
    if signature_valid { Ok(()) } else { Err(ShardSignatureVerificationError::VerificationFailed) }
}

pub(crate) fn try_extract_public_key_from_peer_id(peer_id: &PeerId) -> Option<PublicKey> {
    // Get the multihash from the PeerId
    let multihash = peer_id.as_ref();

    // Check if this is an identity multihash (code 0x00)
    if multihash.code() == 0x00 {
        // For identity multihash, the digest contains the encoded public key
        let encoded_key = multihash.digest();

        // Try to decode the public key from protobuf
        match PublicKey::try_decode_protobuf(encoded_key) {
            Ok(public_key) => {
                // SECURITY: Verify that the extracted key actually matches this PeerId
                // This prevents attacks where someone provides a malicious PeerId
                let derived_peer_id = PeerId::from(&public_key);
                if derived_peer_id == *peer_id {
                    debug!(peer=%peer_id, "Successfully extracted and validated public key from PeerId");
                    Some(public_key)
                } else {
                    warn!(
                        peer=%peer_id,
                        derived_peer=%derived_peer_id,
                        "Security violation: extracted public key does not match PeerId - possible spoofing attempt"
                    );
                    None
                }
            }
            Err(e) => {
                debug!(peer=%peer_id, error=?e, "Failed to decode public key from PeerId");
                None
            }
        }
    } else {
        // This is a hashed PeerId (SHA-256), cannot extract the original key
        debug!(peer=%peer_id, multihash_code=%multihash.code(), "PeerId uses hashed multihash, cannot extract public key");
        None
    }
}

pub(crate) fn validate_public_key_matches_peer_id(
    public_key: &PublicKey,
    peer_id: &PeerId,
) -> bool {
    let derived_peer_id = PeerId::from(public_key);
    derived_peer_id == *peer_id
}

#[cfg(test)]
mod tests {
    use libp2p::identity::Keypair;

    use super::*;

    #[test]
    fn test_sign_and_verify_merkle_root() {
        let keypair = Keypair::generate_ed25519();

        let merkle_root = MessageRoot([
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ]);

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
}
