//! Message types for the Propeller protocol.

use libp2p::core::PeerId;

use crate::types::{Channel, MessageRoot, ShardIndex};
use crate::{MerkleProof, ShardValidationError};

// TODO(AndrewL): consider making fields public and remove
// constructor, getters and setters.

/// A single shard unit in the Propeller protocol.
///
/// Contains one shard of data along with its merkle proof, allowing
/// receivers to verify the shard is part of the original message.
#[derive(Debug, PartialEq, Clone)]
pub struct PropellerUnit {
    channel: Channel,
    publisher: PeerId,
    root: MessageRoot,
    signature: Vec<u8>,
    index: ShardIndex,
    shard: Vec<u8>,
    proof: MerkleProof,
}

impl PropellerUnit {
    pub fn new(
        channel: Channel,
        publisher: PeerId,
        root: MessageRoot,
        signature: Vec<u8>,
        index: ShardIndex,
        shard: Vec<u8>,
        proof: MerkleProof,
    ) -> Self {
        Self { channel, root, publisher, signature, index, shard, proof }
    }

    pub fn channel(&self) -> Channel {
        self.channel
    }

    pub fn publisher(&self) -> PeerId {
        self.publisher
    }

    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    pub fn index(&self) -> ShardIndex {
        self.index
    }

    pub fn shard(&self) -> &[u8] {
        &self.shard
    }

    pub fn shard_mut(&mut self) -> &mut Vec<u8> {
        &mut self.shard
    }

    pub fn proof(&self) -> &MerkleProof {
        &self.proof
    }

    pub fn root(&self) -> MessageRoot {
        self.root
    }

    pub fn validate_shard_proof(&self, num_shards: usize) -> Result<(), ShardValidationError> {
        let proof = self.proof();
        let index = self.index().0.try_into().expect("u32 could not be converted to usize");
        if proof.verify(&self.root().0, &self.shard, index, num_shards) {
            Ok(())
        } else {
            Err(ShardValidationError::MerkleProofVerificationFailed)
        }
    }
}
