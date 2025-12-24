//! Message types for the Propeller protocol.

use libp2p::core::PeerId;

use crate::types::{Channel, MessageRoot, ShardIndex};
use crate::{MerkleProof, MerkleTree, ShardValidationError};

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

    pub fn validate_shard_proof(&self) -> Result<(), ShardValidationError> {
        let proof = self.proof();
        let shard_hash = MerkleTree::hash_leaf(self.shard());
        if proof.verify(&self.root().0, &shard_hash, self.index().0.try_into().unwrap()) {
            Ok(())
        } else {
            Err(ShardValidationError::ProofVerificationFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propeller_unit_creation_and_getters() {
        let channel = Channel(1);
        let publisher = PeerId::random();
        let root = MessageRoot([1u8; 32]);
        let signature = vec![1, 2, 3, 4];
        let index = ShardIndex(0);
        let shard = vec![5, 6, 7, 8];
        let proof = MerkleProof { siblings: vec![[9u8; 32], [10u8; 32]] };

        let unit = PropellerUnit::new(
            channel,
            publisher,
            root,
            signature.clone(),
            index,
            shard.clone(),
            proof.clone(),
        );

        assert_eq!(unit.channel(), channel);
        assert_eq!(unit.publisher(), publisher);
        assert_eq!(unit.signature(), &signature[..]);
        assert_eq!(unit.index(), index);
        assert_eq!(unit.shard(), &shard[..]);
        assert_eq!(unit.proof(), &proof);
        assert_eq!(unit.root(), root);
    }

    #[test]
    fn test_shard_mut() {
        let mut unit = PropellerUnit::new(
            Channel(1),
            PeerId::random(),
            MessageRoot([1u8; 32]),
            vec![],
            ShardIndex(0),
            vec![1, 2, 3],
            MerkleProof { siblings: vec![] },
        );

        let shard = unit.shard_mut();
        shard.push(4);

        assert_eq!(unit.shard(), &[1, 2, 3, 4]);
    }
}
