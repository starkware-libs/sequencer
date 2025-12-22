//! Message types for the Propeller protocol.

use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoPropellerUnit,
};
use libp2p::core::multihash::Multihash;
use libp2p::core::PeerId;
use rand::Rng;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::types::{Channel, MessageRoot, ShardIndex};
use crate::{MerkleHash, MerkleProof, MerkleTree, ShardValidationError};

// Use the protobuf PropellerUnit type directly.
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

    /// Generate a random PropellerUnit for testing.
    pub fn random<R: Rng>(rng: &mut R, max_shard_size: usize) -> Self {
        let channel = Channel(rng.gen());
        let message_id = MessageRoot(rng.gen::<MerkleHash>());
        let proof_len = rng.gen_range(0..=256);
        let proof = MerkleProof { siblings: (0..proof_len).map(|_| rng.gen()).collect() };
        let index = ShardIndex(rng.gen::<u32>());
        let peer_id = rng.gen::<[u8; 32]>();
        let publisher = PeerId::from_multihash(
            Multihash::wrap(0x0, &peer_id).expect("The digest size is never too large"),
        )
        .unwrap();
        let shard_len = rng.gen_range(0..=max_shard_size);
        let mut shard = vec![0u8; shard_len];
        rng.fill(&mut shard[..]);
        let sig_len = rng.gen_range(0..=256);
        let mut signature = vec![0u8; sig_len];
        rng.fill(&mut signature[..]);

        Self::new(channel, publisher, message_id, signature, index, shard, proof)
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
