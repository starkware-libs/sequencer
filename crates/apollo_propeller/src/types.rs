//! Core types for the Propeller protocol.

use crate::MerkleHash;

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
// TODO(AndrewL): rename to ChannelId
// TODO(AndrewL): make it u64 instead of u32
pub struct Channel(pub u32);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct ShardIndex(pub u32);

#[derive(Debug, Default, PartialEq, Clone, Copy, Ord, PartialOrd, Eq, Hash)]
pub struct MessageRoot(pub MerkleHash);
