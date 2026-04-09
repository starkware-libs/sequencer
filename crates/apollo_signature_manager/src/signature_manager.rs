use std::ops::Deref;

use apollo_infra::component_definitions::ComponentStarter;
use apollo_network_types::network_types::PeerId;
use apollo_signature_manager_types::{
    KeySourceConfig,
    KeyStoreError,
    SignatureManagerConfig,
    SignatureManagerError,
    SignatureManagerResult,
};
use async_trait::async_trait;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{
    Challenge,
    PrivateKey,
    PublicKey,
    RawSignature,
    SignatureConversionError,
};
use starknet_core::crypto::{ecdsa_sign, ecdsa_verify, EcdsaVerifyError};
use starknet_core::types::Felt;
use thiserror::Error;

use crate::blake_utils::blake2s_to_felt;

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
pub struct SignatureManager {
    #[allow(dead_code)]
    config: SignatureManagerConfig,
    private_key: PrivateKey,
}

impl SignatureManager {
    pub fn new(config: SignatureManagerConfig) -> Result<Self, SignatureManagerError> {
        let private_key = match &config.key_source {
            KeySourceConfig::Local { private_key } => *private_key,
            KeySourceConfig::GoogleSecretManager { .. } => {
                return Err(SignatureManagerError::KeyStore(KeyStoreError::Custom(
                    "GSM not yet supported".into(),
                )));
            }
        };
        Ok(Self { config, private_key })
    }

    pub async fn sign_identification(
        &self,
        peer_id: PeerId,
        challenge: Challenge,
    ) -> SignatureManagerResult<RawSignature> {
        let message_digest = build_peer_identity_message_digest(peer_id, challenge);
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
        let signature = ecdsa_sign(&self.private_key, &message_digest)
            .map_err(|e| SignatureManagerError::Sign(e.to_string()))?;

        Ok(signature.into())
    }
}

#[async_trait]
impl ComponentStarter for SignatureManager {}

// Utils.
// TODO(noam.s): Consider wrapping each field in fixed delimiters (e.g. parentheses or tags) to
// avoid delimiter ambiguity across implementations; see apollo_propeller/signature.rs and PR
// review.
// TODO(noam.s): replace peer_id with staker_address (or add a new
// build_staker_identity_message_digest function)
fn build_peer_identity_message_digest(peer_id: PeerId, challenge: Challenge) -> MessageDigest {
    let challenge = &challenge.0;
    let peer_id = peer_id.to_bytes();
    let mut message = Vec::with_capacity(INIT_PEER_ID.len() + peer_id.len() + challenge.len());
    message.extend_from_slice(INIT_PEER_ID);
    message.extend_from_slice(&peer_id);
    message.extend_from_slice(challenge);

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
    challenge: Challenge,
    signature: RawSignature,
    public_key: PublicKey,
) -> SignatureVerificationResult<bool> {
    let message_digest = build_peer_identity_message_digest(peer_id, challenge);
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
