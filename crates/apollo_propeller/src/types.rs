//! Core types for the Propeller protocol.

use libp2p::identity::PeerId;
use thiserror::Error;

use crate::padding::UnpaddingError;
use crate::MerkleHash;

// TODO(AndrewL): reduce redundant documentation in this file

// TODO(AndrewL): Re-evaluate the error approach in propeller.

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
    /// Failed to send a shard to a peer.
    ShardSendFailed { sent_from: Option<PeerId>, sent_to: Option<PeerId>, error: ShardPublishError },
    /// Failed to verify shard.
    ShardValidationFailed {
        /// The sender of the shard that failed verification. They should be reported.
        sender: PeerId,
        claimed_root: MessageRoot,
        claimed_publisher: PeerId,
        error: ShardValidationError,
    },
    /// Message processing timed out before completion.
    MessageTimeout { committee_id: CommitteeId, publisher: PeerId, message_root: MessageRoot },
}

// TODO(andrew): add the epoch number to committee ID, so it doesn't repeat if the same members are
// in different epochs.
#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct CommitteeId(pub [u8; 32]);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct ShardIndex(pub u64);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct MessageRoot(pub MerkleHash);

// TODO(Shahak): consider renaming this or refactoring the protobuf so this struct is not needed or
// so it includes all the fields that go into the signature.
#[derive(Debug, Clone)]
pub struct VerifiedFields {
    pub signature: Vec<u8>,
    pub nonce: u64,
}

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
pub enum ScheduleError {
    #[error("Cannot generate tree: publisher {publisher} was not found in the committee")]
    PublisherNotInCommittee {
        /// The publisher that was not found in the peer list.
        publisher: PeerId,
    },
    #[error("Cannot generate tree: the local peer is not included in the committee")]
    LocalPeerNotInCommittee,
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
    ScheduleError(ScheduleError),
    #[error("Committee not registered: {0:?}")]
    CommitteeNotRegistered(CommitteeId),
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
    #[error("Unexpected shard count per unit (expected {expected}, got {actual})")]
    UnexpectedShardCount { expected: usize, actual: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommitteeSetupError {
    #[error("Local peer is not a member in the committee you're requesting")]
    LocalPeerNotInCommittee,
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
    ScheduleManagerError(ScheduleError),
    #[error(
        "Shard failed parent verification (expected sender = {expected_sender}, shard index = \
         {shard_index:?})"
    )]
    UnexpectedSender { expected_sender: PeerId, shard_index: ShardIndex },
    #[error("Shard failed signature verification: {0}")]
    SignatureVerificationFailed(ShardSignatureVerificationError),
    #[error("Shard failed Merkle proof verification")]
    MerkleProofVerificationFailed,
    #[error("Shards have inconsistent lengths")]
    UnequalShardLengths,
    #[error(
        "Unexpected shard count per unit (expected {expected_shard_count}, got \
         {actual_shard_count})"
    )]
    UnexpectedShardCount { expected_shard_count: usize, actual_shard_count: usize },
}
