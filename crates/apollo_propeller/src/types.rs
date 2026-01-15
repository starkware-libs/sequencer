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
