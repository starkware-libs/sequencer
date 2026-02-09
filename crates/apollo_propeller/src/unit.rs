//! Message types for the Propeller protocol.

use apollo_protobuf::converters::ProtobufConversionError;
use apollo_protobuf::protobuf::{
    Hash256 as ProtoHash256, MerkleProof as ProtoMerkleProof, PeerId as ProtoPeerId,
    PropellerUnit as ProtoPropellerUnit,
};
use libp2p::core::PeerId;

use crate::types::{Channel, MessageRoot, ShardIndex};
use crate::{MerkleHash, MerkleProof, ShardValidationError};

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

impl TryFrom<ProtoPropellerUnit> for PropellerUnit {
    type Error = ProtobufConversionError;

    fn try_from(msg: ProtoPropellerUnit) -> Result<Self, Self::Error> {
        let publisher_id = msg
            .publisher
            .ok_or(ProtobufConversionError::MissingField { field_description: "publisher" })?;
        let merkle_proof = msg
            .merkle_proof
            .ok_or(ProtobufConversionError::MissingField { field_description: "merkle_proof" })?;
        let index: u32 =
            msg.index.try_into().map_err(|_| ProtobufConversionError::OutOfRangeValue {
                type_description: "PropellerUnit.index",
                value_as_str: msg.index.to_string(),
            })?;
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

        Ok(Self {
            channel: Channel(msg.channel),
            root: MessageRoot(merkle_root_bytes),
            publisher: PeerId::from_bytes(&publisher_id.id).map_err(|e| {
                ProtobufConversionError::OutOfRangeValue {
                    type_description: "PeerId",
                    value_as_str: e.to_string(),
                }
            })?,
            signature: msg.signature,
            index: ShardIndex(index),
            shard: msg.shard,
            proof: merkle_proof.try_into()?,
        })
    }
}

impl From<PropellerUnit> for ProtoPropellerUnit {
    fn from(msg: PropellerUnit) -> Self {
        ProtoPropellerUnit {
            shard: msg.shard,
            index: msg.index.0.into(),
            merkle_root: Some(ProtoHash256 { elements: msg.root.0.to_vec() }),
            merkle_proof: Some((&msg.proof).into()),
            publisher: Some(ProtoPeerId { id: msg.publisher.to_bytes() }),
            signature: msg.signature,
            channel: msg.channel.0,
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
