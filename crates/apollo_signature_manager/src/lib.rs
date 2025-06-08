use apollo_signature_manager_types::{
    KeyStore,
    PeerId,
    SignatureManagerError,
    SignatureManagerResult,
};
use blake2s::blake2s_to_felt;
use starknet_api::crypto::utils::{Message, PublicKey, RawSignature};
use starknet_core::crypto::ecdsa_verify;
use starknet_core::types::Felt;

// Message domain separators.
pub(crate) const INIT_PEER_ID: &[u8] = b"INIT_PEER_ID";

#[derive(Debug, Default, derive_more::Deref, Eq, PartialEq, Hash)]
struct MessageDigest(pub Felt);

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

fn build_peer_identity_message_digest(peer_id: PeerId) -> MessageDigest {
    let mut message = Vec::with_capacity(INIT_PEER_ID.len() + peer_id.len());
    message.extend_from_slice(INIT_PEER_ID);
    message.extend_from_slice(&peer_id);

    MessageDigest(blake2s_to_felt(&message))
}

// Library functions for signature verification.

/// Verification is a local operation, not requiring access to a keystore (hence remote
/// communication).
/// It is also much faster than signing, so that clients can invoke a simple library call.
pub fn verify_identity(
    peer_id: PeerId,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureManagerResult<bool> {
    let message_digest = build_peer_identity_message_digest(peer_id);
    let signature = signature.try_into()?;

    ecdsa_verify(&public_key, &message_digest, &signature)
        .map_err(|error| SignatureManagerError::Verify(error.to_string()))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use starknet_api::felt;
    use starknet_core::crypto::Signature;

    use super::*;

    #[rstest]
    #[case::valid_signature(
        Signature {
            r: felt!("0x606f47b45330e70c562306d037079eaeb0e07050dfb731be743556e796152e3"),
            s: felt!("0x2644bef4418c2a3fcfd4d6b48e66f9c10b88884ffec6608d5d4b312024d6aa5"),
        },
        true
    )]
    #[case::invalid_signature(
        Signature { r: felt!("0x1"), s: felt!("0x2") },
        false
    )]
    fn test_verify_identity(#[case] signature: Signature, #[case] expected: bool) {
        let peer_id = PeerId(b"alice".to_vec());
        let public_key =
            PublicKey(felt!("0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a"));
        assert_eq!(verify_identity(peer_id, signature.into(), public_key), Ok(expected));
    }
}
