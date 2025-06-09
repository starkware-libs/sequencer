use std::ops::Deref;

use apollo_signature_manager_types::{
    KeyStore,
    PeerId,
    SignatureManagerError,
    SignatureManagerResult,
};
use blake2s::blake2s_to_felt;
use starknet_api::block::BlockHash;
use starknet_api::crypto::utils::{PublicKey, RawSignature};
use starknet_core::crypto::ecdsa_verify;
use starknet_core::types::Felt;

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

/// Provides signing and signature verification functionality.
pub struct SignatureManager<KS: KeyStore> {
    pub keystore: KS,
}

impl<KS: KeyStore> SignatureManager<KS> {
    fn _new(keystore: KS) -> Self {
        Self { keystore }
    }

    pub async fn identify(&self, _peer_id: PeerId) -> SignatureManagerResult<RawSignature> {
        todo!("SignatureManager::identify is not implemented yet");
    }

    pub async fn sign_precommit_vote(
        &self,
        _block_hash: BlockHash,
    ) -> SignatureManagerResult<RawSignature> {
        todo!("SignatureManager::sign_precommit_vote is not implemented yet");
    }
}

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
}
