//! Core types for the Propeller protocol.

use std::fmt::Display;

use libp2p::identity::PeerId;

use crate::MerkleHash;

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

/// Events emitted by the Propeller behaviour.
#[derive(Debug, Clone)]
pub enum Event {
    /// A shard has been received from a peer.
    ShardReceived {
        /// The publisher of the shard.
        publisher: PeerId,
        /// The index of the shard.
        shard_index: ShardIndex,
        /// The merkle root of the message.
        message_root: MessageRoot,
        /// The peer that sent the shard.
        sender: PeerId,
        /// The received shard.
        shard: Vec<u8>,
    },
    /// A complete message has been reconstructed from shards.
    MessageReceived {
        /// The publisher of the message.
        publisher: PeerId,
        /// The merkle root of the message.
        message_root: MessageRoot,
        /// The reconstructed message data.
        message: Vec<u8>,
    },
    /// Failed to reconstruct a message from shards.
    MessageReconstructionFailed {
        /// The merkle root of the message.
        message_root: MessageRoot,
        /// The publisher of the shard.
        publisher: PeerId,
        /// The error that occurred.
        error: ReconstructionError,
    },
    /// Failed to send a shard to a peer.
    ShardSendFailed {
        /// The peer we sent the shard from.
        sent_from: Option<PeerId>,
        /// The peer we sent the shard to.
        sent_to: Option<PeerId>,
        /// The error that occurred.
        error: ShardPublishError,
    },
    /// Failed to verify shard
    ShardValidationFailed {
        /// The sender of the shard that filed verification.
        ///
        /// They should probably be reported
        sender: PeerId,
        /// The stated publisher of the shard, might not have verified yet.
        claimed_root: MessageRoot,
        /// The claimed publisher of the shard.
        claimed_publisher: PeerId,
        /// The specific verification error that occurred.
        error: ShardValidationError,
    },
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
        }
    }
}

impl std::error::Error for ShardPublishError {}

// ****************************************************************************

/// Errors that can occur during message reconstruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconstructionError {
    /// Erasure reconstruction failed.
    ErasureReconstructionFailed(String),
    /// The message root of the data + coding shards does not match the one provided by the
    /// publisher, possible attack.
    MismatchedMessageRoot,
    /// Unequal shard lengths
    UnequalShardLengths,
    /// Message padding error.
    MessagePaddingError,
}

impl std::fmt::Display for ReconstructionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconstructionError::ErasureReconstructionFailed(msg) => {
                write!(f, "Erasure reconstruction failed: {}", msg)
            }
            ReconstructionError::MismatchedMessageRoot => {
                write!(f, "Mismatched message root, the publisher is most likely malicious")
            }
            ReconstructionError::UnequalShardLengths => {
                write!(f, "Unequal shard lengths, the shards are most likely malicious")
            }
            ReconstructionError::MessagePaddingError => {
                write!(f, "The message was padded incorrectly by the publisher")
            }
        }
    }
}

impl std::error::Error for ReconstructionError {}

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

/// Specific errors that can occur during shard verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardValidationError {
    /// Publisher should not receive their own shards (they broadcast them).
    ReceivedPublishedShard,
    /// Shard is already in cache (duplicate).
    DuplicateShard,
    /// Failed to get parent in tree topology.
    TreeError(TreeGenerationError),
    /// Shard received from an unexpected sender.
    UnexpectedSender {
        /// The expected sender
        expected_sender: PeerId,
        /// The index of the shard that was received from the unexpected sender.
        shard_index: ShardIndex,
    },
    /// Shard signature verification failed.
    SignatureVerificationFailed(ShardSignatureVerificationError),
    /// Shard proof verification failed.
    ProofVerificationFailed,
}

impl std::fmt::Display for ShardValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardValidationError::ReceivedPublishedShard => {
                write!(f, "Publisher should not receive their own shard")
            }
            ShardValidationError::DuplicateShard => {
                write!(f, "Received shard that is already in cache")
            }
            ShardValidationError::TreeError(e) => {
                write!(f, "Received shard but error getting parent in tree: {}", e)
            }
            ShardValidationError::UnexpectedSender { expected_sender, shard_index } => {
                write!(
                    f,
                    "Shard failed parent verification (expected sender = {}, shard index = {})",
                    expected_sender, shard_index
                )
            }
            ShardValidationError::SignatureVerificationFailed(e) => {
                write!(f, "Shard failed signature verification: {}", e)
            }
            ShardValidationError::ProofVerificationFailed => {
                write!(f, "Shard failed proof verification")
            }
        }
    }
}

impl std::error::Error for ShardValidationError {}

// ****************************************************************************

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerSetError {
    LocalPeerNotInPeerWeights,
    InvalidPublicKey,
}

impl std::fmt::Display for PeerSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerSetError::LocalPeerNotInPeerWeights => write!(f, "Local peer not in peer weights"),
            PeerSetError::InvalidPublicKey => write!(f, "Invalid public key"),
        }
    }
}

impl std::error::Error for PeerSetError {}
