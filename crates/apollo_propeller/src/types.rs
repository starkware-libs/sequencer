//! Core types for the Propeller protocol.

use libp2p::identity::PeerId;
use thiserror::Error;

use crate::padding::UnpaddingError;
use crate::MerkleHash;

// TODO(AndrewL): reduce redundant documentation in this file

/// Events emitted by the Propeller protocol to the application layer.
#[derive(Debug, Clone)]
pub enum Event {
    /// A complete message has been reconstructed from shards.
    MessageReceived { publisher: PeerId, message_root: MessageRoot, message: Vec<u8> },
    /// Failed to reconstruct a message from shards.
    MessageReconstructionFailed {
        message_root: MessageRoot,
        publisher: PeerId,
        error: ReconstructionError,
    },
    // TODO(AndrewL): remove this and just use ShardSendFailed
    /// Failed to broadcast a shard.
    ShardPublishFailed { error: ShardPublishError },
    /// Failed to send a shard to a peer.
    ShardSendFailed { sent_from: Option<PeerId>, sent_to: Option<PeerId>, error: ShardPublishError },
    /// Failed to verify shard
    ShardValidationFailed {
        /// The sender of the shard that filed verification. They should be reported.
        sender: PeerId,
        claimed_root: MessageRoot,
        claimed_publisher: PeerId,
        error: ShardValidationError,
    },
    /// Message processing timed out before completion.
    MessageTimeout { channel: Channel, publisher: PeerId, message_root: MessageRoot },
}

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
// TODO(AndrewL): rename TreeGenerationError
pub enum TreeGenerationError {
    #[error("Cannot generate tree: publisher {publisher} was not found in the channel")]
    PublisherNotInChannel {
        /// The publisher that was not found in the peer list.
        publisher: PeerId,
    },
    #[error("Cannot generate tree: the local peer is not included in the channel")]
    LocalPeerNotInChannel,
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
    // TODO(AndrewL): add a proper error type for the handler error.
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
    #[error("The message was padded incorrectly by the publisher: {0}")]
    MessagePaddingError(UnpaddingError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
// TODO(AndrewL): rename to ChannelSetupError
pub enum PeerSetError {
    #[error("Local peer is not a member in the channel you're requesting")]
    LocalPeerNotInChannel,
    #[error("Invalid public key")]
    InvalidPublicKey,
    #[error("Duplicate peer IDs")]
    DuplicatePeerIds,
}

/// Specific errors that can occur during shard verification.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShardValidationError {
    #[error("Self received a shard from myself (libp2p should not allow this)")]
    SelfSending,
    #[error("Publisher should not receive their own shard")]
    ReceivedSelfPublishedShard,
    #[error("Received shard that is already in cache (duplicate)")]
    DuplicateShard,
    #[error("Received shard but error getting parent in tree topology: {0}")]
    ScheduleManagerError(TreeGenerationError),
    #[error(
        "Shard failed parent verification (expected sender = {expected_sender}, shard index = \
         {shard_index:?})"
    )]
    UnexpectedSender { expected_sender: PeerId, shard_index: ShardIndex },
    #[error("Shard failed signature verification: {0}")]
    SignatureVerificationFailed(ShardSignatureVerificationError),
    #[error("Shard failed Merkle proof verification")]
    MerkleProofVerificationFailed,
}
