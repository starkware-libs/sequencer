//! Cryptographic utilities.
//! This module provides cryptographic utilities.
#[cfg(test)]
#[path = "crypto_test.rs"]
#[allow(clippy::explicit_auto_deref)]
mod crypto_test;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, Poseidon, StarkHash as CoreStarkHash};
use thiserror::Error;

use crate::hash::StarkHash;

/// An error that can occur during cryptographic operations.

#[derive(thiserror::Error, Clone, Debug)]
pub enum CryptoError {
    #[error("Invalid public key {0:#x}.")]
    InvalidPublicKey(PublicKey),
    #[error("Invalid message hash {0:#x}.")]
    InvalidMessageHash(Felt),
    #[error("Invalid r {0}.")]
    InvalidR(Felt),
    #[error("Invalid s {0}.")]
    InvalidS(Felt),
}

/// A public key.
#[derive(
    Debug, Default, derive_more::Deref, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize,
)]
pub struct PublicKey(pub Felt);

impl std::fmt::LowerHex for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::LowerHex::fmt(&self.0, f)
    }
}

/// A private key.
#[derive(
    Debug, Default, derive_more::Deref, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize,
)]
pub struct PrivateKey(pub Felt);

/// A signature.
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct Signature {
    pub r: Felt,
    pub s: Felt,
}

/// Verifies the authenticity of a signed message hash given the public key of the signer.
pub fn verify_message_hash_signature(
    message_hash: &Felt,
    signature: &Signature,
    public_key: &PublicKey,
) -> Result<bool, CryptoError> {
    starknet_crypto::verify(&public_key.0, message_hash, &signature.r, &signature.s).map_err(
        |err| match err {
            starknet_crypto::VerifyError::InvalidPublicKey => {
                CryptoError::InvalidPublicKey(*public_key)
            }
            starknet_crypto::VerifyError::InvalidMessageHash => {
                CryptoError::InvalidMessageHash(*message_hash)
            }
            starknet_crypto::VerifyError::InvalidR => CryptoError::InvalidR(signature.r),
            starknet_crypto::VerifyError::InvalidS => CryptoError::InvalidS(signature.s),
        },
    )
}

// Collect elements for applying hash chain.
pub(crate) struct HashChain {
    elements: Vec<Felt>,
}

impl HashChain {
    pub fn new() -> HashChain {
        HashChain { elements: Vec::new() }
    }

    // Chains a felt to the hash chain.
    pub fn chain(mut self, felt: &Felt) -> Self {
        self.elements.push(*felt);
        self
    }

    // Chains the result of a function to the hash chain.
    pub fn chain_if_fn<F: Fn() -> Option<Felt>>(self, f: F) -> Self {
        match f() {
            Some(felt) => self.chain(&felt),
            None => self,
        }
    }

    // Chains many felts to the hash chain.
    pub fn chain_iter<'a>(self, felts: impl Iterator<Item = &'a Felt>) -> Self {
        felts.fold(self, |current, felt| current.chain(felt))
    }

    // Chains the number of felts followed by the felts themselves to the hash chain.
    pub fn chain_size_and_elements(self, felts: &[Felt]) -> Self {
        self.chain(&felts.len().into()).chain_iter(felts.iter())
    }

    // Chains a chain of felts to the hash chain.
    pub fn extend(mut self, chain: HashChain) -> Self {
        self.elements.extend(chain.elements);
        self
    }

    // Returns the pedersen hash of the chained felts, hashed with the length of the chain.
    pub fn get_pedersen_hash(&self) -> StarkHash {
        Pedersen::hash_array(self.elements.as_slice())
    }

    // Returns the poseidon hash of the chained felts.
    pub fn get_poseidon_hash(&self) -> StarkHash {
        Poseidon::hash_array(self.elements.as_slice())
    }
}

/// A raw message to be signed, represented as a sequence of field elements (`Felt`).
///
/// Before signing, the message vector should be hashed to produce a digest suitable
/// for the signature algorithm in use (e.g., Blake hash).
#[derive(
    Debug, Default, derive_more::Deref, Clone, Eq, PartialEq, Hash, Deserialize, Serialize,
)]
pub struct Message(pub Vec<Felt>);

/// A raw Stark signature, represented as a list of field elements (`Felt`).
///
/// Depending on the signature scheme, this typically contains two elements
/// (e.g., `(r, s)` for ECDSA-like schemes), but may vary for other algorithms.
///
/// This generic container allows higher-level traits to work without the burden of
/// propagating generic signature types.
#[derive(Clone, Debug, derive_more::Deref, Eq, PartialEq, Serialize, Deserialize)]
pub struct RawSignature(pub Vec<Felt>);

impl From<starknet_crypto::Signature> for RawSignature {
    fn from(signature: starknet_crypto::Signature) -> Self {
        Self(vec![signature.r, signature.s])
    }
}

#[derive(Clone, Debug, Error, Serialize, Deserialize, Eq, PartialEq)]
pub enum SignatureConversionError {
    #[error("expected a 2-element signature, but got length {0}")]
    InvalidLength(usize),
}

impl TryFrom<RawSignature> for starknet_crypto::Signature {
    type Error = SignatureConversionError;

    fn try_from(signature: RawSignature) -> Result<Self, Self::Error> {
        let signature_length = signature.len();
        let [r, s]: [Felt; 2] = signature
            .0
            .try_into()
            .map_err(|_| SignatureConversionError::InvalidLength(signature_length))?;

        Ok(starknet_crypto::Signature { r, s })
    }
}
