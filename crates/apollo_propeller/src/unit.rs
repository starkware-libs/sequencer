//! Message types for the Propeller protocol.

use libp2p::core::multihash::Multihash;
use libp2p::core::PeerId;
use rand::Rng;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::generated::propeller::pb as proto;
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

impl TryFrom<proto::PropellerUnit> for PropellerUnit {
    type Error = std::io::Error;

    fn try_from(msg: proto::PropellerUnit) -> Result<Self, Self::Error> {
        Ok(Self {
            channel: Channel(msg.channel),
            root: MessageRoot(msg.root.try_into().map_err(|e: Vec<u8>| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid message root length: {}", e.len()),
                )
            })?),
            publisher: PeerId::from_bytes(&msg.publisher)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
            signature: msg.signature,
            index: ShardIndex(msg.index),
            shard: msg.shard,
            proof: MerkleProof::deserialize(&msg.proof)?,
        })
    }
}

impl From<PropellerUnit> for proto::PropellerUnit {
    fn from(msg: PropellerUnit) -> Self {
        proto::PropellerUnit {
            channel: msg.channel.0,
            root: msg.root.0.to_vec(),
            publisher: msg.publisher.to_bytes(),
            signature: msg.signature,
            index: msg.index.0,
            shard: msg.shard,
            proof: msg.proof.serialize(),
        }
    }
}
