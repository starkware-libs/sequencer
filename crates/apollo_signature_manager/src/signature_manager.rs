use std::ops::Deref;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::{
    KeyStore,
    KeyStoreResult,
    SignatureManagerError,
    SignatureManagerResult,
};
use async_trait::async_trait;
use blake2s::blake2s_to_felt;
use starknet_api::block::BlockHash;
use starknet_api::core::Nonce;
use starknet_api::crypto::utils::{PrivateKey, PublicKey, RawSignature, SignatureConversionError};
use starknet_core::crypto::{ecdsa_sign, ecdsa_verify, EcdsaVerifyError};
use starknet_core::types::Felt;
use starknet_crypto::get_public_key;
use thiserror::Error;

#[cfg(test)]
#[path = "signature_manager_test.rs"]
pub mod signature_manager_test;

// Message domain separators.
pub(crate) const INIT_PEER_ID: &[u8] = b"INIT_PEER_ID";
pub(crate) const PRECOMMIT_VOTE: &[u8] = b"PRECOMMIT_VOTE";

pub type SignatureVerificationResult<T> = Result<T, SignatureVerificationError>;

#[derive(Debug, Default, Eq, PartialEq, Hash)]
struct MessageDigest(pub Felt);

impl Deref for MessageDigest {
    type Target = Felt;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Provides signing and signature verification functionality.
#[derive(Clone, Debug)]
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    pub fn new(keystore: KS) -> Self {
        Self { keystore }
    }

    // TODO(Elin): sign both peer IDs.
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
        block_hash: BlockHash,
    ) -> SignatureManagerResult<RawSignature> {
        let message_digest = build_precommit_vote_message_digest(block_hash);
        self.sign(message_digest).await
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

/// A simple in-memory key store.
#[derive(Clone, Copy, Debug)]
pub struct LocalKeyStore {
    pub public_key: PublicKey,
    private_key: PrivateKey,
}

impl LocalKeyStore {
    fn _new(private_key: PrivateKey) -> Self {
        let public_key = PublicKey(get_public_key(&private_key));
        Self { private_key, public_key }
    }

    pub(crate) const fn new_for_testing() -> Self {
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

#[async_trait]
impl KeyStore for LocalKeyStore {
    async fn get_key(&self) -> KeyStoreResult<PrivateKey> {
        Ok(self.private_key)
    }
}

// Utils.

fn build_peer_identity_message_digest(peer_id: PeerId, nonce: Nonce) -> MessageDigest {
    let nonce = nonce.to_bytes_be();
    let peer_id = peer_id.to_bytes();
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
) -> SignatureVerificationResult<bool> {
    Ok(ecdsa_verify(&public_key, &message_digest, &signature.try_into()?)?)
}

// Library functions for signature verification.

/// Verification is a local operation, not requiring access to a keystore (hence remote
/// communication).
/// It is also much faster than signing, so that clients can invoke a simple library call.

#[derive(Debug, Error)]
pub enum SignatureVerificationError {
    #[error(transparent)]
    EcdsaVerify(#[from] EcdsaVerifyError),
    #[error(transparent)]
    SignatureConversion(#[from] SignatureConversionError),
}

pub fn verify_identity(
    peer_id: PeerId,
    nonce: Nonce,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureVerificationResult<bool> {
    let message_digest = build_peer_identity_message_digest(peer_id, nonce);
    verify_signature(message_digest, signature, public_key)
}

pub fn verify_precommit_vote_signature(
    block_hash: BlockHash,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureVerificationResult<bool> {
    let message_digest = build_precommit_vote_message_digest(block_hash);
    verify_signature(message_digest, signature, public_key)
}
