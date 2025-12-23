//! Core types for the Propeller protocol.

use libp2p::identity::PeerId;
use thiserror::Error;

use crate::MerkleHash;

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
// TODO(AndrewL): rename to ChannelId
// TODO(AndrewL): make it u64 instead of u32
pub struct Channel(pub u32);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct ShardIndex(pub u32);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct MessageRoot(pub MerkleHash);

/// Errors that can occur when verifying a shard signature.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShardSignatureVerificationError {
    #[error(
        "We could not find a public key for signer/publisher {0}. This suggests that the public \
         key cannot be extracted form the peer ID and needs to be provided explicitly."
    )]
    NoPublicKeyAvailable(PeerId),
    #[error("Received a shard with an invalid signature. Sender should be reported...")]
    VerificationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TreeGenerationError {
    #[error("Cannot generate tree: publisher {publisher} was not found in the peer list")]
    PublisherNotFound {
        /// The publisher that was not found in the peer list.
        publisher: PeerId,
    },
    #[error("Cannot generate tree: the local peer is not included in the peer weights")]
    LocalPeerNotInPeerWeights,
    #[error(
        "Cannot generate tree: shard index {:?} is out of bounds. Might be out of sync with peers.",
        .shard_index
    )]
    ShardIndexOutOfBounds { shard_index: ShardIndex },
    #[error("Cannot generate tree: the local peer is the publisher.")]
    LocalPeerIsPublisher,
}

/// Errors that can occur when sending a shard.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShardPublishError {
    #[error("Local peer not in peer weights")]
    LocalPeerNotInPeerWeights,
    #[error(
        "Invalid data size for broadcasting, data size must be divisible by number of data shards"
    )]
    InvalidDataSize,
    #[error("Signing failed: {0}")]
    SigningFailed(String),
    #[error("Erasure encoding failed: {0}")]
    ErasureEncodingFailed(String),
    #[error("Not connected to peer {0}")]
    NotConnectedToPeer(PeerId),
    #[error("Handler error: {0}")]
    HandlerError(String),
    #[error("Tree generation error: {0}")]
    TreeGenerationError(TreeGenerationError),
    #[error("Channel not registered: {0:?}")]
    ChannelNotRegistered(Channel),
    #[error("Broadcast failed to complete")]
    BroadcastFailed,
}

/// Errors that can occur during message reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReconstructionError {
    #[error("Erasure reconstruction failed: {0}")]
    ErasureReconstructionFailed(String),
    #[error("Mismatched message root, the publisher is most likely malicious")]
    MismatchedMessageRoot,
    #[error("Unequal shard lengths, the shards are most likely malicious")]
    UnequalShardLengths,
    #[error("The message was padded incorrectly by the publisher")]
    MessagePaddingError,
}
