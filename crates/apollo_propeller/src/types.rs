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

// ****************************************************************************

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeGenerationError {
    PublisherNotFound {
        /// The publisher that was not found in the peer list.
        publisher: PeerId,
    },
    LocalPeerNotInPeerWeights,
    ShardIndexOutOfBounds {
        shard_index: ShardIndex,
    },
    LocalPeerIsPublisher,
}

impl std::fmt::Display for TreeGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeGenerationError::PublisherNotFound { publisher } => {
                write!(f, "Publisher not found: {}", publisher)
            }
            TreeGenerationError::LocalPeerNotInPeerWeights => {
                write!(f, "Local peer not in peer weights")
            }
            TreeGenerationError::ShardIndexOutOfBounds { shard_index } => {
                write!(f, "Shard index out of bounds: {}", shard_index)
            }
            TreeGenerationError::LocalPeerIsPublisher => {
                write!(f, "Local peer is publisher")
            }
        }
    }
}

impl std::error::Error for TreeGenerationError {}

// ****************************************************************************

/// Errors that can occur when sending a shard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardPublishError {
    LocalPeerNotInPeerWeights,
    InvalidDataSize,
    SigningFailed(String),
    ErasureEncodingFailed(String),
    NotConnectedToPeer(PeerId),
    HandlerError(String),
    TreeGenerationError(TreeGenerationError),
    ChannelNotRegistered(Channel),
    BroadcastFailed,
}

impl std::fmt::Display for ShardPublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardPublishError::LocalPeerNotInPeerWeights => {
                write!(f, "Local peer not in peer weights")
            }
            ShardPublishError::InvalidDataSize => {
                write!(
                    f,
                    "Invalid data size for broadcasting, data size must be divisible by number of \
                     data shards"
                )
            }
            ShardPublishError::SigningFailed(e) => {
                write!(f, "Signing failed: {}", e)
            }
            ShardPublishError::ErasureEncodingFailed(e) => {
                write!(f, "Erasure encoding failed: {}", e)
            }
            ShardPublishError::NotConnectedToPeer(peer_id) => {
                write!(f, "Not connected to peer {}", peer_id)
            }
            ShardPublishError::HandlerError(e) => {
                write!(f, "Handler error: {}", e)
            }
            ShardPublishError::TreeGenerationError(e) => {
                write!(f, "Tree generation error: {}", e)
            }
            ShardPublishError::ChannelNotRegistered(channel) => {
                write!(f, "Channel not registered: {}", channel)
            }
            ShardPublishError::BroadcastFailed => {
                write!(f, "Broadcast failed to complete")
            }
        }
    }
}

impl std::error::Error for ShardPublishError {}
