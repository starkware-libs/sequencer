//! Cryptographic utilities.
//! This module provides cryptographic utilities.
#[cfg(test)]
#[path = "crypto_test.rs"]
mod crypto_test;

use serde::{Deserialize, Serialize};
use starknet_crypto::{pedersen_hash, poseidon_hash_many, FieldElement};

use crate::hash::{StarkFelt, StarkHash};

/// An error that can occur during cryptographic operations.
#[derive(thiserror::Error, Clone, Debug)]
pub enum CryptoError {
    #[error("Invalid public key {0:?}.")]
    InvalidPublicKey(PublicKey),
    #[error("Invalid message hash {0:?}.")]
    InvalidMessageHash(StarkFelt),
    #[error("Invalid r {0:?}.")]
    InvalidR(StarkFelt),
    #[error("Invalid s {0:?}.")]
    InvalidS(StarkFelt),
}

/// A public key.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct PublicKey(pub StarkFelt);

/// A signature.
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct Signature {
    pub r: StarkFelt,
    pub s: StarkFelt,
}

/// Verifies the authenticity of a signed message hash given the public key of the signer.
pub fn verify_message_hash_signature(
    message_hash: &StarkFelt,
    signature: &Signature,
    public_key: &PublicKey,
) -> Result<bool, CryptoError> {
    starknet_crypto::verify(
        &public_key.0.into(),
        &FieldElement::from(*message_hash),
        &signature.r.into(),
        &signature.s.into(),
    )
    .map_err(|err| match err {
        starknet_crypto::VerifyError::InvalidPublicKey => {
            CryptoError::InvalidPublicKey(*public_key)
        }
        starknet_crypto::VerifyError::InvalidMessageHash => {
            CryptoError::InvalidMessageHash(*message_hash)
        }
        starknet_crypto::VerifyError::InvalidR => CryptoError::InvalidR(signature.r),
        starknet_crypto::VerifyError::InvalidS => CryptoError::InvalidS(signature.s),
    })
}

// Collect elements for applying hash chain.
pub(crate) struct HashChain {
    elements: Vec<FieldElement>,
}

impl HashChain {
    pub fn new() -> HashChain {
        HashChain { elements: Vec::new() }
    }

    // Chains a felt to the hash chain.
    pub fn chain(mut self, felt: &StarkFelt) -> Self {
        self.elements.push(FieldElement::from(*felt));
        self
    }

    // Chains the result of a function to the hash chain.
    pub fn chain_if_fn<F: Fn() -> Option<StarkFelt>>(self, f: F) -> Self {
        match f() {
            Some(felt) => self.chain(&felt),
            None => self,
        }
    }

    // Chains many felts to the hash chain.
    pub fn chain_iter<'a>(self, felts: impl Iterator<Item = &'a StarkFelt>) -> Self {
        felts.fold(self, |current, felt| current.chain(felt))
    }

    // Chains the number of felts followed by the felts themselves to the hash chain.
    pub fn chain_size_and_elements(self, felts: &[StarkFelt]) -> Self {
        self.chain(&felts.len().into()).chain_iter(felts.iter())
    }

    // Returns the pedersen hash of the chained felts, hashed with the length of the chain.
    pub fn get_pedersen_hash(&self) -> StarkHash {
        let current_hash = self
            .elements
            .iter()
            .fold(FieldElement::ZERO, |current_hash, felt| pedersen_hash(&current_hash, felt));
        let n_elements = FieldElement::from(self.elements.len());
        pedersen_hash(&current_hash, &n_elements).into()
    }

    // Returns the poseidon hash of the chained felts.
    pub fn get_poseidon_hash(&self) -> StarkHash {
        poseidon_hash_many(&self.elements).into()
    }
}
