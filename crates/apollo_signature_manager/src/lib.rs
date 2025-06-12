pub mod communication;

use std::ops::Deref;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_signature_manager_types::{
    KeyStore,
    KeyStoreResult,
    PeerId,
    SignatureManagerError,
    SignatureManagerResult,
};
use async_trait::async_trait;
use blake2s::blake2s_to_felt;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{PrivateKey, PublicKey, RawSignature};
use starknet_core::crypto::{ecdsa_sign, ecdsa_verify};
use starknet_core::types::Felt;
use starknet_crypto::get_public_key;

// Message domain separators.
pub(crate) const INIT_PEER_ID: &[u8] = b"INIT_PEER_ID";
pub(crate) const PRECOMMIT_VOTE: &[u8] = b"PRECOMMIT_VOTE";

#[derive(Debug, Default, Eq, PartialEq, Hash)]
struct MessageDigest(pub Felt);

impl Deref for MessageDigest {
    type Target = Felt;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// Simple in-memory KeyStore implementation for testing
#[cfg(test)] // Better than allow dead code, since it won't compile in first out-of-tests usage.
#[derive(Clone, Copy, Debug)]
struct LocalKeyStore {
    private_key: PrivateKey,
    pub public_key: PublicKey,
}

#[cfg(test)]
impl LocalKeyStore {
    fn _new(private_key: PrivateKey) -> Self {
        let public_key = PublicKey(get_public_key(&private_key.0));
        Self { private_key, public_key }
    }

    #[cfg(test)]
    const fn new_for_testing() -> Self {
        // Created using `cairo-lang`.
        const PRIVATE_KEY: PrivateKey = PrivateKey(Felt::from_hex_unchecked(
            "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133",
        ));
        const PUBLIC_KEY: PublicKey = PublicKey(Felt::from_hex_unchecked(
            "0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a",
        ));

        Self { private_key: PRIVATE_KEY, public_key: PUBLIC_KEY }
    }
}

#[cfg(test)]
#[async_trait]
impl KeyStore for LocalKeyStore {
    async fn get_key(&self) -> KeyStoreResult<PrivateKey> {
        Ok(self.private_key)
    }
}

/// Provides signing and signature verification functionality.
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    pub fn new(keystore: KS) -> Self {
        Self { keystore }
    }

    pub async fn identify(&self, peer_id: PeerId) -> SignatureManagerResult<RawSignature> {
        let message_digest = build_peer_identity_message_digest(peer_id);
        self.sign(message_digest).await
    }

    pub async fn sign_precommit_vote(
        &self,
        _block_hash: BlockHash,
    ) -> SignatureManagerResult<RawSignature> {
        todo!("SignatureManager::sign_precommit_vote is not implemented yet");
    }

    async fn sign(&self, message_digest: MessageDigest) -> SignatureManagerResult<RawSignature> {
        let private_key = self.keystore.get_key().await?;
        let signature = ecdsa_sign(&private_key, &message_digest)
            .map_err(|e| SignatureManagerError::Sign(e.to_string()))?;

        Ok(signature.into())
    }
}

#[async_trait]
impl<KS: KeyStore> ComponentStarter for SignatureManager<KS> {}

// Utils.

fn build_peer_identity_message_digest(peer_id: PeerId) -> MessageDigest {
    let mut message = Vec::with_capacity(INIT_PEER_ID.len() + peer_id.len());
    message.extend_from_slice(INIT_PEER_ID);
    message.extend_from_slice(&peer_id);

    MessageDigest(blake2s_to_felt(&message))
}

fn build_precommit_vote_message_digest(block_hash: BlockHash) -> MessageDigest {
    let block_hash = block_hash.to_bytes_be();
    let mut message = Vec::with_capacity(PRECOMMIT_VOTE.len() + block_hash.len());
    message.extend_from_slice(PRECOMMIT_VOTE);
    message.extend_from_slice(&block_hash);

    MessageDigest(blake2s_to_felt(&message))
}

fn verify_signature(
    message_digest: MessageDigest,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureManagerResult<bool> {
    ecdsa_verify(&public_key, &message_digest, &signature.try_into()?)
        .map_err(|e| SignatureManagerError::Verify(e.to_string()))
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
    verify_signature(message_digest, signature, public_key)
}

pub fn verify_precommit_vote_signature(
    block_hash: BlockHash,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureManagerResult<bool> {
    let message_digest = build_precommit_vote_message_digest(block_hash);
    verify_signature(message_digest, signature, public_key)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use starknet_api::felt;
    use starknet_core::crypto::Signature;
    use starknet_core::types::Felt;

    use super::*;

    const ALICE_IDENTITY_SIGNATURE: Signature = Signature {
        r: Felt::from_hex_unchecked(
            "0x606f47b45330e70c562306d037079eaeb0e07050dfb731be743556e796152e3",
        ),
        s: Felt::from_hex_unchecked(
            "0x2644bef4418c2a3fcfd4d6b48e66f9c10b88884ffec6608d5d4b312024d6aa5",
        ),
    };

    #[rstest]
    #[case::valid_signature(ALICE_IDENTITY_SIGNATURE, true)]
    #[case::invalid_signature(
        Signature { r: felt!("0x1"), s: felt!("0x2") },
        false
    )]
    fn test_verify_identity(#[case] signature: Signature, #[case] expected: bool) {
        let peer_id = PeerId(b"alice".to_vec());
        let public_key = LocalKeyStore::new_for_testing().public_key;

        assert_eq!(verify_identity(peer_id, signature.into(), public_key), Ok(expected));
    }

    #[rstest]
    #[case::valid_signature(
        Signature {
            r: felt!("0xcd59947811bac7c33d3dae3d50b1de243710b05f285455ada6823e23871a2b"),
            s: felt!("0x33817fd47c5253c4979999afe0dd6b275498d9c7b96dd7705b84c2113228f11"),
        },
        true
    )]
    #[case::invalid_signature(
        Signature { r: felt!("0x1"), s: felt!("0x2") },
        false
    )]
    fn test_verify_precommit_vote_signature(#[case] signature: Signature, #[case] expected: bool) {
        let block_hash = BlockHash(felt!("0x1234"));
        let public_key = LocalKeyStore::new_for_testing().public_key;

        assert_eq!(
            verify_precommit_vote_signature(block_hash, signature.into(), public_key),
            Ok(expected)
        );
    }

    #[tokio::test]
    async fn test_identify() {
        let key_store = LocalKeyStore::new_for_testing();
        let signature_manager = SignatureManager::new(key_store);

        let peer_id = PeerId(b"alice".to_vec());
        let signature = signature_manager.identify(peer_id.clone()).await;

        assert_eq!(signature, Ok(ALICE_IDENTITY_SIGNATURE.into()));

        // Test alignment with verification function.
        assert_eq!(verify_identity(peer_id, signature.unwrap(), key_store.public_key), Ok(true));
    }
}
