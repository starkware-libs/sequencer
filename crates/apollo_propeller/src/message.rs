//! Message types for the Propeller protocol.

use asynchronous_codec::{Decoder, Encoder};
use bytes::BytesMut;
use libp2p::core::multihash::Multihash;
use libp2p::core::PeerId;
use quick_protobuf::MessageWrite;
use rand::Rng;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::generated::propeller::pb as proto;
use crate::protocol::PropellerCodec;
use crate::types::{MessageRoot, ShardIndex};
use crate::{MerkleHash, MerkleProof, MerkleTree, ShardValidationError};

// Use the protobuf PropellerMessage type directly.
#[derive(Debug, PartialEq, Clone)]
pub struct PropellerMessage {
    root: MessageRoot,
    publisher: PeerId,
    signature: Vec<u8>,
    index: ShardIndex,
    shard: Vec<u8>,
    proof: MerkleProof,
}

impl PropellerMessage {
    pub fn new(
        root: MessageRoot,
        publisher: PeerId,
        signature: Vec<u8>,
        index: ShardIndex,
        shard: Vec<u8>,
        proof: MerkleProof,
    ) -> Self {
        Self { root, publisher, signature, index, shard, proof }
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

    pub fn proof(&self) -> &MerkleProof {
        &self.proof
    }

    pub fn root(&self) -> MessageRoot {
        self.root
    }

    /// Calculate the hash of this message.
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        let mut dst = BytesMut::new();
        let proto: proto::PropellerMessage = self.clone().into();
        self.encode(&mut dst, proto.get_size());
        hasher.update(dst);
        hasher.finalize().into()
    }

    /// Generate a random PropellerMessage for testing.
    pub fn random<R: Rng>(rng: &mut R, max_shard_size: usize) -> Self {
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
        let shard: Vec<u8> = (0..shard_len).map(|_| rng.gen()).collect();
        let sig_len = rng.gen_range(0..=256);
        let signature: Vec<u8> = (0..sig_len).map(|_| rng.gen()).collect();

        Self::new(message_id, publisher, signature, index, shard, proof)
    }

    pub fn encode(&self, dst: &mut BytesMut, max_message_size: usize) {
        let mut codec = PropellerCodec::new(max_message_size);
        codec.encode(self.clone(), dst).unwrap();
    }

    pub fn decode(src: &mut BytesMut, max_message_size: usize) -> Option<Self> {
        let mut codec = PropellerCodec::new(max_message_size);
        codec.decode(src).ok().flatten()
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

impl TryFrom<proto::PropellerMessage> for PropellerMessage {
    type Error = std::io::Error;

    fn try_from(msg: proto::PropellerMessage) -> Result<Self, Self::Error> {
        if msg.root.len() != 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message root length: {}", msg.root.len()),
            ));
        }
        Ok(Self {
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

impl From<PropellerMessage> for proto::PropellerMessage {
    fn from(msg: PropellerMessage) -> Self {
        proto::PropellerMessage {
            root: msg.root.0.to_vec(),
            publisher: msg.publisher.to_bytes(),
            signature: msg.signature,
            index: msg.index.0,
            shard: msg.shard,
            proof: msg.proof.serialize(),
        }
    }
}
