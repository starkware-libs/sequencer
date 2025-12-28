//! Core types for the Propeller protocol.

use std::fmt::Display;

use libp2p::identity::PeerId;

use crate::MerkleHash;

// ****************************************************************************

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct Channel(pub u32);

impl Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Channel({})", self.0)
    }
}

// ****************************************************************************

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct ShardIndex(pub u32);

impl Display for ShardIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ShardIndex({})", self.0)
    }
}

// ****************************************************************************

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct MessageRoot(pub MerkleHash);

impl Display for MessageRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MessageRoot(0x",)?;
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        write!(f, ")")
    }
}

// ****************************************************************************

/// Errors that can occur when verifying a shard signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardSignatureVerificationError {
    NoPublicKeyAvailable(PeerId),
    EmptySignature,
    VerificationFailed,
}

impl std::fmt::Display for ShardSignatureVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardSignatureVerificationError::NoPublicKeyAvailable(publisher) => {
                write!(f, "No public key available for signer {}", publisher)
            }
            ShardSignatureVerificationError::EmptySignature => {
                write!(f, "Shard has empty signature")
            }
            ShardSignatureVerificationError::VerificationFailed => {
                write!(f, "Shard signature is invalid")
            }
        }
    }
}

impl std::error::Error for ShardSignatureVerificationError {}
