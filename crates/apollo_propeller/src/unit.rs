//! Message types for the Propeller protocol.

use apollo_protobuf::converters::ProtobufConversionError;
use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256,
    MerkleProof as ProtoMerkleProof,
    PeerId as ProtoPeerId,
    PropellerUnit as ProtoPropellerUnit,
    Shard as ProtoShard,
    ShardsOfPeer as ProtoShardsOfPeer,
};
use libp2p::core::PeerId;
use prost::Message;

use crate::types::{CommitteeId, MessageRoot, UnitIndex};
use crate::{MerkleHash, MerkleProof, UnitValidationError};

/// A single erasure-coded fragment.
#[derive(Debug, PartialEq, Clone)]
pub struct Shard(pub Vec<u8>);

/// A collection of shards assigned to a single peer.
///
/// The Merkle tree leaf for this peer is computed by hashing the proto-encoded bytes
/// of this struct, ensuring cross-language determinism.
#[derive(Debug, PartialEq, Clone)]
pub struct ShardsOfPeer(pub Vec<Shard>);

impl ShardsOfPeer {
    /// Encode as proto bytes for use as a Merkle tree leaf.
    pub fn encode_to_proto_bytes(&self) -> Vec<u8> {
        let proto: ProtoShardsOfPeer = self.into();
        proto.encode_to_vec()
    }
}

impl From<&ShardsOfPeer> for ProtoShardsOfPeer {
    fn from(shards: &ShardsOfPeer) -> Self {
        ProtoShardsOfPeer {
            shards: shards.0.iter().map(|s| ProtoShard { data: s.0.clone() }).collect(),
        }
    }
}

impl From<ProtoShardsOfPeer> for ShardsOfPeer {
    fn from(proto: ProtoShardsOfPeer) -> Self {
        ShardsOfPeer(proto.shards.into_iter().map(|s| Shard(s.data)).collect())
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
    committee_id: CommitteeId,
    publisher: PeerId,
    root: MessageRoot,
    signature: Vec<u8>,
    index: UnitIndex,
    shards: ShardsOfPeer,
    proof: MerkleProof,
    /// Any strictly increasing number.
    /// Current implementation is nanoseconds since UNIX_EPOCH.
    nonce: u64,
}

impl PropellerUnit {
    // TODO(guyn): consider removing this constructor entirely and initializing the struct directly.
    // Need fields to be public.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        committee_id: CommitteeId,
        publisher: PeerId,
        root: MessageRoot,
        signature: Vec<u8>,
        index: UnitIndex,
        shards: ShardsOfPeer,
        proof: MerkleProof,
        nonce: u64,
    ) -> Self {
        Self { committee_id, root, publisher, signature, index, shards, proof, nonce }
    }

    pub fn committee_id(&self) -> CommitteeId {
        self.committee_id
    }

    pub fn publisher(&self) -> PeerId {
        self.publisher
    }

    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    pub fn index(&self) -> UnitIndex {
        self.index
    }

    pub fn shards(&self) -> &ShardsOfPeer {
        &self.shards
    }

    pub fn shards_mut(&mut self) -> &mut ShardsOfPeer {
        &mut self.shards
    }

    pub fn proof(&self) -> &MerkleProof {
        &self.proof
    }

    pub fn root(&self) -> MessageRoot {
        self.root
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn validate_shard_count(
        &self,
        expected_shard_count: usize,
    ) -> Result<(), UnitValidationError> {
        let actual_shard_count = self.shards.0.len();
        if actual_shard_count != expected_shard_count {
            return Err(UnitValidationError::UnexpectedShardCount {
                expected_shard_count,
                actual_shard_count,
            });
        }
        Ok(())
    }

    pub fn validate_shard_lengths(&self) -> Result<(), UnitValidationError> {
        let Some(first) = self.shards.0.first() else {
            return Ok(());
        };
        let expected_len = first.0.len();
        if self.shards.0.iter().any(|s| s.0.len() != expected_len) {
            return Err(UnitValidationError::UnequalShardLengths);
        }
        Ok(())
    }

    pub fn validate_merkle_proof(&self, num_total_units: usize) -> Result<(), UnitValidationError> {
        let proof = self.proof();
        let index = self.index().0.try_into().expect("u64 could not be converted to usize");
        // Encode as proto bytes because the Merkle tree leaves are the proto-encoded bytes
        // of `ShardsOfPeer`, ensuring cross-language determinism.
        let leaf_data = self.shards.encode_to_proto_bytes();
        if proof.verify(&self.root().0, &leaf_data, index, num_total_units) {
            Ok(())
        } else {
            Err(UnitValidationError::MerkleProofVerificationFailed)
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
        let committee_id_bytes: [u8; 32] = msg
            .committee_id
            .ok_or(ProtobufConversionError::MissingField { field_description: "committee_id" })?
            .elements
            .try_into()
            .map_err(|e: Vec<u8>| ProtobufConversionError::BytesDataLengthMismatch {
                type_description: "CommitteeId",
                num_expected: 32,
                value: e,
            })?;
        let proto_shards = msg
            .shards
            .ok_or(ProtobufConversionError::MissingField { field_description: "shards" })?;

        Ok(Self {
            committee_id: CommitteeId(committee_id_bytes),
            root: MessageRoot(merkle_root_bytes),
            publisher: PeerId::from_bytes(&publisher_id.id).map_err(|e| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "PeerId",
                    value_as_str: e.to_string(),
                }
            })?,
            signature: msg.signature,
            index: UnitIndex(msg.index),
            shards: ShardsOfPeer::from(proto_shards),
            proof: merkle_proof.try_into()?,
            nonce: msg.nonce,
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
            committee_id: Some(ProtoHash256 { elements: msg.committee_id.0.to_vec() }),
            nonce: msg.nonce,
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
