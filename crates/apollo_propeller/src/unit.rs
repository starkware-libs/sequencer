//! Message types for the Propeller protocol.

use apollo_protobuf::converters::ProtobufConversionError;
use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoPropellerUnit,
    Shard as ProtoShard,
    Shards as ProtoShards,
};
use libp2p::core::PeerId;
use prost::Message;

use crate::types::{Committee, MessageRoot, ShardIndex};
use crate::{MerkleHash, MerkleProof, ShardValidationError};

/// A single erasure-coded fragment.
#[derive(Debug, PartialEq, Clone)]
pub struct Shard(pub Vec<u8>);

/// A collection of shards assigned to a single peer.
///
/// The Merkle tree leaf for this peer is computed by hashing the proto-encoded bytes
/// of this struct, ensuring cross-language determinism.
#[derive(Debug, PartialEq, Clone)]
pub struct Shards(pub Vec<Shard>);

impl Shards {
    /// Encode as proto bytes for use as a Merkle tree leaf.
    pub fn encode_to_proto_bytes(&self) -> Vec<u8> {
        let proto: ProtoShards = self.into();
        proto.encode_to_vec()
    }
}

impl From<&Shards> for ProtoShards {
    fn from(shards: &Shards) -> Self {
        ProtoShards { shards: shards.0.iter().map(|s| ProtoShard { data: s.0.clone() }).collect() }
    }
}

impl From<ProtoShards> for Shards {
    fn from(proto: ProtoShards) -> Self {
        Shards(proto.shards.into_iter().map(|s| Shard(s.data)).collect())
    }
}

// TODO(AndrewL): consider making fields public and remove
// constructor, getters and setters.

/// A single unit in the Propeller protocol.
///
/// Contains shards of data along with a merkle proof, allowing
/// receivers to verify the shards are part of the original message.
#[derive(Debug, PartialEq, Clone)]
pub struct PropellerUnit {
    committee: Committee,
    publisher: PeerId,
    root: MessageRoot,
    signature: Vec<u8>,
    index: ShardIndex,
    shards: Shards,
    proof: MerkleProof,
}

impl PropellerUnit {
    pub fn new(
        committee: Committee,
        publisher: PeerId,
        root: MessageRoot,
        signature: Vec<u8>,
        index: ShardIndex,
        shards: Shards,
        proof: MerkleProof,
    ) -> Self {
        Self { committee, root, publisher, signature, index, shards, proof }
    }

    pub fn committee(&self) -> Committee {
        self.committee
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

    pub fn shards(&self) -> &Shards {
        &self.shards
    }

    pub fn shards_mut(&mut self) -> &mut Shards {
        &mut self.shards
    }

    pub fn proof(&self) -> &MerkleProof {
        &self.proof
    }

    pub fn root(&self) -> MessageRoot {
        self.root
    }

    pub fn validate_shard_proof(&self, num_shards: usize) -> Result<(), ShardValidationError> {
        let proof = self.proof();
        let index = self.index().0.try_into().expect("u64 could not be converted to usize");
        let leaf_data = self.shards.encode_to_proto_bytes();
        if proof.verify(&self.root().0, &leaf_data, index, num_shards) {
            Ok(())
        } else {
            Err(ShardValidationError::MerkleProofVerificationFailed)
        }
    }
}

impl TryFrom<ProtoPropellerUnit> for PropellerUnit {
    type Error = ProtobufConversionError;

    fn try_from(msg: ProtoPropellerUnit) -> Result<Self, Self::Error> {
        let publisher_id = msg
            .publisher
            .ok_or(ProtobufConversionError::MissingField { field_description: "publisher" })?;
        let merkle_proof = msg
            .merkle_proof
            .ok_or(ProtobufConversionError::MissingField { field_description: "merkle_proof" })?;
        let merkle_root_bytes: [u8; 32] = msg
            .merkle_root
            .ok_or(ProtobufConversionError::MissingField { field_description: "merkle_root" })?
            .elements
            .try_into()
            .map_err(|e: Vec<u8>| ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "MessageRoot",
                num_expected: 32,
                value: e,
            })?;
        let committee_bytes: [u8; 32] = msg
            .committee
            .ok_or(ProtobufConversionError::MissingField { field_description: "committee" })?
            .elements
            .try_into()
            .map_err(|e: Vec<u8>| ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "Committee",
                num_expected: 32,
                value: e,
            })?;
        let proto_shards = msg
            .shards
            .ok_or(ProtobufConversionError::MissingField { field_description: "shards" })?;

        Ok(Self {
            committee: Committee(committee_bytes),
            root: MessageRoot(merkle_root_bytes),
            publisher: PeerId::from_bytes(&publisher_id.id).map_err(|e| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "PeerId",
                    value_as_str: e.to_string(),
                }
            })?,
            signature: msg.signature,
            index: ShardIndex(msg.index),
            shards: Shards::from(proto_shards),
            proof: merkle_proof.try_into()?,
        })
    }
}

impl From<PropellerUnit> for ProtoPropellerUnit {
    fn from(msg: PropellerUnit) -> Self {
        ProtoPropellerUnit {
            shards: Some((&msg.shards).into()),
            index: msg.index.0,
            merkle_root: Some(ProtoHash256 { elements: msg.root.0.to_vec() }),
            merkle_proof: Some((&msg.proof).into()),
            publisher: Some(ProtoPeerId { id: msg.publisher.to_bytes() }),
            signature: msg.signature,
            committee: Some(ProtoHash256 { elements: msg.committee.0.to_vec() }),
        }
    }
}

impl TryFrom<ProtoMerkleProof> for MerkleProof {
    type Error = ProtobufConversionError;

    fn try_from(proto: ProtoMerkleProof) -> Result<Self, Self::Error> {
        let mut siblings = Vec::with_capacity(proto.siblings.len());
        for sibling in &proto.siblings {
            let hash: MerkleHash = sibling.elements.as_slice().try_into().map_err(|_| {
                ProtobufConversionError::BytesDataLengthMismatch {
                    type_description: "MerkleHash",
                    num_expected: 32,
                    value: sibling.elements.clone(),
                }
            })?;
            siblings.push(hash);
        }
        Ok(MerkleProof { siblings })
    }
}

impl From<&MerkleProof> for ProtoMerkleProof {
    fn from(proof: &MerkleProof) -> Self {
        ProtoMerkleProof {
            siblings: proof
                .siblings
                .iter()
                .map(|h| ProtoHash256 { elements: h.to_vec() })
                .collect(),
        }
    }
}
