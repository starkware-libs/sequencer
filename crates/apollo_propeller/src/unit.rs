//! Message types for the Propeller protocol.

use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoPropellerUnit,
};
use libp2p::core::PeerId;

use crate::types::{Channel, MessageRoot, ShardIndex};
use crate::{MerkleHash, MerkleProof, MerkleTree, ShardValidationError};

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

impl TryFrom<ProtoPropellerUnit> for PropellerUnit {
    type Error = std::io::Error;

    fn try_from(msg: ProtoPropellerUnit) -> Result<Self, Self::Error> {
        let publisher_id = msg.publisher.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing publisher")
        })?;
        let merkle_proof = msg.merkle_proof.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing merkle_proof")
        })?;
        let index: u32 = msg.index.try_into().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Index too large for u32")
        })?;
        let merkle_root_bytes: [u8; 32] = msg
            .merkle_root
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing merkle_root")
            })?
            .elements
            .try_into()
            .map_err(|e: Vec<u8>| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid message root length: {}", e.len()),
                )
            })?;

        Ok(Self {
            channel: Channel(msg.channel),
            root: MessageRoot(merkle_root_bytes),
            publisher: PeerId::from_bytes(&publisher_id.id)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            signature: msg.signature,
            index: ShardIndex(index),
            shard: msg.shard,
            proof: proto_merkle_proof_to_merkle_proof(&merkle_proof)?,
        })
    }
}

impl From<PropellerUnit> for ProtoPropellerUnit {
    fn from(msg: PropellerUnit) -> Self {
        ProtoPropellerUnit {
            shard: msg.shard,
            index: msg.index.0.into(),
            merkle_root: Some(ProtoHash256 { elements: msg.root.0.to_vec() }),
            merkle_proof: Some(merkle_proof_to_proto(&msg.proof)),
            publisher: Some(ProtoPeerId { id: msg.publisher.to_bytes() }),
            signature: msg.signature,
            channel: msg.channel.0,
        }
    }
}

/// Convert a proto MerkleProof to a MerkleProof.
fn proto_merkle_proof_to_merkle_proof(
    proto: &ProtoMerkleProof,
) -> Result<MerkleProof, std::io::Error> {
    let mut siblings = Vec::with_capacity(proto.siblings.len());
    for sibling in &proto.siblings {
        let hash: MerkleHash = sibling.elements.as_slice().try_into().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Invalid merkle proof sibling length: {} (expected 32)",
                    sibling.elements.len()
                ),
            )
        })?;
        siblings.push(hash);
    }
    Ok(MerkleProof { siblings })
}

/// Convert a MerkleProof to a proto MerkleProof.
fn merkle_proof_to_proto(proof: &MerkleProof) -> ProtoMerkleProof {
    ProtoMerkleProof {
        siblings: proof.siblings.iter().map(|h| ProtoHash256 { elements: h.to_vec() }).collect(),
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
