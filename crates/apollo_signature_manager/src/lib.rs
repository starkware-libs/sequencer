use apollo_signature_manager_types::{KeyStore, SignatureManagerError, SignatureManagerResult};
use blake2s::blake2s_to_felt;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use starknet_core::crypto::ecdsa_verify;
use starknet_core::types::Felt;

// Message domain separators.
pub(crate) const INIT_PEER_ID: &[u8] = b"INIT_PEER_ID";

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct PeerID(Vec<u8>);

#[derive(Debug, Default, Eq, PartialEq, Hash)]
pub(crate) struct MessageDigest(pub Felt);

/// Provides signing and signature verification functionality.
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    fn _new(keystore: KS) -> Self {
        Self { keystore }
    }

    pub async fn sign(&self, _message: Message) -> SignatureManagerResult<RawSignature> {
        todo!("SignatureManager::sign is not yet implemented");
    }

    pub async fn verify(
        &self,
        _signature: RawSignature,
        _message: Message,
        _public_key: PublicKey,
    ) -> SignatureManagerResult<bool> {
        todo!("SignatureManager::verify is not yet implemented");
    }
}

// Utils.

fn build_peer_identity_message_digest(peer_id: PeerID) -> MessageDigest {
    let mut message = Vec::with_capacity(INIT_PEER_ID.len() + peer_id.0.len());
    message.extend_from_slice(INIT_PEER_ID);
    message.extend_from_slice(&peer_id.0);

    MessageDigest(blake2s_to_felt(&message))
}

// Library functions for signature verification.

/// Verification is a local operation, not requiring access to a keystore (hence remote
/// communication).
/// It is also much faster than signing, so that clients can invoke a simple library call.
pub fn verify_identity(
    peer_id: PeerID,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureManagerResult<bool> {
    let message_digest = build_peer_identity_message_digest(peer_id);
    let signature = signature.try_into()?;

    ecdsa_verify(&public_key.0, &message_digest.0, &signature)
        .map_err(|error| SignatureManagerError::Verify(error.to_string()))
}
