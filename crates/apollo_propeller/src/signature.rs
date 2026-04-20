//! Signature creation and validation for the Propeller protocol.
//!
//! This module handles cryptographic operations for signing and verifying messages,
//! following similar patterns to gossipsub for consistency with the libp2p ecosystem.

use libp2p::identity::{Keypair, PeerId, PublicKey};

use crate::types::{CommitteeId, MessageRoot, ShardSignatureVerificationError, UnitPublishError};

// TODO(AndrewL): Consider removing these (consult gossipsub code )
pub const SIGNING_PREFIX: &[u8] = b"<propeller>";
pub const SIGNING_POSTFIX: &[u8] = b"</propeller>";

/// Multihash code for identity hash (inline key in PeerId)
const IDENTITY_MULTIHASH_CODE: u64 = 0x00;

pub fn sign_message_id(
    message_id: &MessageRoot,
    committee_id: CommitteeId,
    nonce: u64,
    keypair: &Keypair,
) -> Result<Vec<u8>, UnitPublishError> {
    let msg = build_signed_payload(message_id, committee_id, nonce);
    // TODO(AndrewL): Use a transparent error type for this.
    keypair.sign(&msg).map_err(|e| UnitPublishError::SigningFailed(e.to_string()))
}

pub fn verify_message_id_signature(
    message_id: &MessageRoot,
    committee_id: CommitteeId,
    nonce: u64,
    signature: &[u8],
    public_key: &PublicKey,
) -> Result<(), ShardSignatureVerificationError> {
    if signature.is_empty() {
        return Err(ShardSignatureVerificationError::VerificationFailed);
    }
    let msg = build_signed_payload(message_id, committee_id, nonce);
    let signature_valid = public_key.verify(&msg, signature);
    if signature_valid { Ok(()) } else { Err(ShardSignatureVerificationError::VerificationFailed) }
}

pub fn try_extract_public_key_from_peer_id(peer_id: &PeerId) -> Option<PublicKey> {
    // Get the multihash from the PeerId
    let multihash = peer_id.as_ref();

    // Check if this is an identity multihash (code 0x00)
    if multihash.code() == IDENTITY_MULTIHASH_CODE {
        // For identity multihash, the digest contains the encoded public key
        let encoded_key = multihash.digest();

        // Try to decode the public key from protobuf
        match PublicKey::try_decode_protobuf(encoded_key) {
            Ok(public_key) => {
                // SECURITY: Verify that the extracted key actually matches this PeerId
                // This prevents attacks where someone provides a malicious PeerId
                let derived_peer_id = PeerId::from(&public_key);
                if derived_peer_id == *peer_id {
                    tracing::trace!(peer=%peer_id, "Successfully extracted and validated public key from PeerId");
                    Some(public_key)
                } else {
                    tracing::warn!(
                        peer=%peer_id,
                        derived_peer=%derived_peer_id,
                        "Security violation: extracted public key does not match PeerId - possible spoofing attempt"
                    );
                    None
                }
            }
            Err(e) => {
                tracing::trace!(peer=%peer_id, error=?e, "Failed to decode public key from PeerId");
                None
            }
        }
    } else {
        // This is a hashed PeerId (SHA-256), cannot extract the original key
        tracing::trace!(peer=%peer_id, multihash_code=%multihash.code(), "PeerId uses hashed multihash, cannot extract public key");
        None
    }
}

// TODO(AndrewL): Consider moving this to test module if not used elsewhere.
pub fn validate_public_key_matches_peer_id(public_key: &PublicKey, peer_id: &PeerId) -> bool {
    let derived_peer_id = PeerId::from(public_key);
    derived_peer_id == *peer_id
}

fn build_signed_payload(
    message_id: &MessageRoot,
    committee_id: CommitteeId,
    nonce: u64,
) -> Vec<u8> {
    [SIGNING_PREFIX, &message_id.0, &committee_id.0, &nonce.to_be_bytes(), SIGNING_POSTFIX].concat()
}
