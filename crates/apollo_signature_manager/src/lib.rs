use apollo_signature_manager_types::{
    KeyStore,
    PeerId,
    SignatureManagerError,
    SignatureManagerResult,
};
use blake2s::blake2s_to_felt;
use starknet_api::block::BlockHash;
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use starknet_core::crypto::{ecdsa_sign, ecdsa_verify};
use starknet_core::types::Felt;

// Message domain separators.
pub(crate) const INIT_PEER_ID: &[u8] = b"INIT_PEER_ID";
pub(crate) const PRECOMMIT_VOTE: &[u8] = b"PRECOMMIT_VOTE";

#[derive(Debug, Default, derive_more::Deref, Eq, PartialEq, Hash)]
struct MessageDigest(pub Felt);

/// Provides signing and signature verification functionality.
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    pub fn new(keystore: KS) -> Self {
        Self { keystore }
    }

    pub async fn identify(
        &self,
        peer_id: PeerId,
        nonce: Nonce, // Used to challenge identity signatures.
    ) -> SignatureManagerResult<RawSignature> {
        let message_digest = build_peer_identity_message_digest(peer_id, nonce);
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

// Utils.

fn build_peer_identity_message_digest(peer_id: PeerId, nonce: Nonce) -> MessageDigest {
    let nonce = nonce.to_bytes_be();
    let mut message = Vec::with_capacity(INIT_PEER_ID.len() + peer_id.len() + nonce.len());
    message.extend_from_slice(INIT_PEER_ID);
    message.extend_from_slice(&peer_id);
    message.extend_from_slice(&nonce);

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
    nonce: Nonce,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureManagerResult<bool> {
    let message_digest = build_peer_identity_message_digest(peer_id, nonce);
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
    use apollo_signature_manager_types::KeyStoreResult;
    use async_trait::async_trait;
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use starknet_api::crypto::utils::PrivateKey;
    use starknet_api::{felt, nonce};
    use starknet_core::crypto::Signature;
    use starknet_core::types::Felt;

    use super::*;

    const ALICE_IDENTITY_SIGNATURE: Signature = Signature {
        r: Felt::from_hex_unchecked(
            "0x7687c83bdfa7474518c585f1b58a028b939764f1d2721e63bf821c4c8987299",
        ),
        s: Felt::from_hex_unchecked(
            "0x7e05746545ed1fe24fec988341d2452a4bbcebec26d73f9ee9bdc9426a372a5",
        ),
    };

    #[rstest]
    #[case::valid_signature(ALICE_IDENTITY_SIGNATURE, true)]
    #[case::invalid_signature(
        Signature { r: felt!("0x1"), s: felt!("0x2") },
        false
    )]
    fn test_verify_identity(#[case] signature: Signature, #[case] expected: bool) {
        let PeerIdentity { peer_id, nonce } = PeerIdentity::new();
        let public_key =
            PublicKey(felt!("0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a"));

        assert_eq!(verify_identity(peer_id, nonce, signature.into(), public_key), Ok(expected));
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
        let public_key =
            PublicKey(felt!("0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a"));

        assert_eq!(
            verify_precommit_vote_signature(block_hash, signature.into(), public_key),
            Ok(expected)
        );
    }

    #[derive(Clone, Debug)]
    struct PeerIdentity {
        pub peer_id: PeerId,
        pub nonce: Nonce,
    }

    impl PeerIdentity {
        pub fn new() -> Self {
            Self { peer_id: PeerId(b"alice".to_vec()), nonce: nonce!(0x1234) }
        }
    }

    /// Simple in-memory KeyStore implementation for testing
    #[derive(Clone, Copy, Debug)]
    struct TestKeyStore {
        private_key: PrivateKey,
        public_key: PublicKey,
    }

    impl TestKeyStore {
        fn new() -> Self {
            // Created using `cairo-lang`.
            let private_key = PrivateKey(felt!(
                "0x608bf2cdb1ad4138e72d2f82b8c5db9fa182d1883868ae582ed373429b7a133"
            ));
            let public_key = PublicKey(felt!(
                "0x125d56b1fbba593f1dd215b7c55e384acd838cad549c4a2b9c6d32d264f4e2a"
            ));

            Self { private_key, public_key }
        }

        fn get_public_key(&self) -> PublicKey {
            self.public_key
        }
    }

    #[async_trait]
    impl KeyStore for TestKeyStore {
        async fn get_key(&self) -> KeyStoreResult<PrivateKey> {
            Ok(self.private_key)
        }
    }

    #[tokio::test]
    async fn test_identify() {
        let key_store = TestKeyStore::new();
        let signature_manager = SignatureManager::new(key_store);

        let PeerIdentity { peer_id, nonce } = PeerIdentity::new();
        let signature = signature_manager.identify(peer_id.clone(), nonce).await;

        assert_eq!(signature, Ok(ALICE_IDENTITY_SIGNATURE.into()));

        // Test alignment with verification function.
        let public_key = key_store.get_public_key();
        assert_eq!(verify_identity(peer_id, nonce, signature.unwrap(), public_key), Ok(true));
    }
}
