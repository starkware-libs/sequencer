#[cfg(test)]
#[path = "consensus_test.rs"]
mod consensus_test;

use std::fmt::Display;

use bytes::{Buf, BufMut};
use prost::DecodeError;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::crypto::utils::RawSignature;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::hash::StarkHash;
use starknet_types_core::felt::Felt;

use crate::converters::ProtobufConversionError;

pub type Round = u32;

#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Display,
    derive_more::Deref,
)]
pub struct ProposalCommitment(pub StarkHash);

pub trait IntoFromProto: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> {}
impl<T> IntoFromProto for T where
    T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>
{
}

#[derive(Debug, Default, Hash, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoteType {
    Prevote,
    #[default]
    Precommit,
}

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Vote {
    pub vote_type: VoteType,
    pub height: BlockNumber,
    pub round: Round,
    pub proposal_commitment: Option<ProposalCommitment>,
    pub voter: ContractAddress,
    pub signature: RawSignature,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum StreamMessageBody<T> {
    Content(T),
    Fin,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct StreamMessage<T: IntoFromProto, StreamId: IntoFromProto + Clone> {
    pub message: StreamMessageBody<T>,
    pub stream_id: StreamId,
    pub message_id: u64,
}

/// Contains the minimal information needed to start building a proposal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildParam {
    /// The height of the consensus (block number).
    pub height: BlockNumber,
    /// The current round of the consensus.
    pub round: Round,
    /// The last round that was valid.
    pub valid_round: Option<Round>,
    /// Address of the one who proposed the block.
    pub proposer: ContractAddress,
}

/// This message must be sent first when proposing a new block.
/// This struct differs from `BlockInfo` in `starknet_api` because we send L1 gas prices in ETH and
/// include the ETH to STRK conversion rate. This allows for more informative validations, as we can
/// distinguish whether an issue comes from the L1 price reading or the conversion rate instead of
/// comparing after multiplication.
#[derive(Clone, Debug, PartialEq)]
pub struct ProposalInit {
    /// The height of the consensus (block number).
    pub height: BlockNumber,
    /// The current round of the consensus.
    pub round: Round,
    /// The last round that was valid.
    pub valid_round: Option<Round>,
    /// Address of the one who proposed the block in consensus.
    pub proposer: ContractAddress,
    /// Block timestamp.
    pub timestamp: u64,
    /// Address of the one who builds/sequences the block.
    pub builder: ContractAddress,
    /// L1 data availability mode.
    pub l1_da_mode: L1DataAvailabilityMode,
    /// L2 gas price in FRI.
    pub l2_gas_price_fri: GasPrice,
    /// L1 gas price in FRI.
    pub l1_gas_price_fri: GasPrice,
    /// L1 data gas price in FRI.
    pub l1_data_gas_price_fri: GasPrice,
    // Keeping the wei prices for now, to use with L1 transactions.
    /// L1 gas price in WEI.
    pub l1_gas_price_wei: GasPrice,
    /// L1 data gas price in WEI.
    pub l1_data_gas_price_wei: GasPrice,
    /// Starknet protocol version.
    pub starknet_version: starknet_api::block::StarknetVersion,
    /// Version constant commitment.
    pub version_constant_commitment: StarkHash,
}

/// A temporary constant to use as a validator ID. Zero is not a valid contract address.
// TODO(Matan): Remove this once we have a proper validator set.
pub const DEFAULT_VALIDATOR_ID: u64 = 100;

impl Default for BuildParam {
    fn default() -> Self {
        BuildParam {
            height: Default::default(),
            round: Default::default(),
            valid_round: Default::default(),
            proposer: ContractAddress::from(DEFAULT_VALIDATOR_ID),
        }
    }
}

impl Default for ProposalInit {
    fn default() -> Self {
        ProposalInit {
            height: Default::default(),
            round: Default::default(),
            valid_round: Default::default(),
            proposer: ContractAddress::from(DEFAULT_VALIDATOR_ID),
            builder: ContractAddress::from(DEFAULT_VALIDATOR_ID),
            timestamp: Default::default(),
            l1_da_mode: L1DataAvailabilityMode::Calldata,
            l2_gas_price_fri: Default::default(),
            l1_gas_price_fri: Default::default(),
            l1_data_gas_price_fri: Default::default(),
            l1_gas_price_wei: Default::default(),
            l1_data_gas_price_wei: Default::default(),
            starknet_version: starknet_api::block::StarknetVersion::LATEST,
            version_constant_commitment: Default::default(),
        }
    }
}

/// There is one or more batches of transactions in a proposed block.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionBatch {
    /// The transactions in the batch.
    pub transactions: Vec<ConsensusTransaction>,
}

/// Optional parts of a commitment carried in ProposalFin.
#[derive(Debug, Clone, PartialEq)]
pub struct CommitmentParts {
    pub next_l2_gas_price_fri: GasPrice,
    pub concatenated_counts: Felt,
    pub parent_commitment: ProposalCommitment,
}

/// The proposal is done when receiving this fin message, which contains the proposal commitment.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalFin {
    /// The commitment identifying the proposed block.
    /// TODO(Matan): Consider changing the content ID to a signature.
    pub proposal_commitment: ProposalCommitment,
    /// Number of executed transactions in the proposal.
    pub executed_transaction_count: u64,
    /// Optional commitment parts.
    pub commitment_parts: Option<CommitmentParts>,
}

/// A part of the proposal.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalPart {
    /// The init part of the proposal (block metadata).
    Init(ProposalInit),
    /// Identifies the content of the proposal; contains `id(v)` in Tendermint terms.
    Fin(ProposalFin),
    /// A part of the proposal that contains one or more transactions.
    Transactions(TransactionBatch),
}

/// A proposal part with a signature (wire format).
#[derive(Debug, Clone, PartialEq)]
pub struct SignedProposalPart {
    pub part: ProposalPart,
    pub signature: RawSignature,
}
// TODO(Asmaa): Delete this once we sign the proposal parts.
impl SignedProposalPart {
    /// Creates a signed init part with default (empty) signature.
    pub fn init(init: ProposalInit) -> Self {
        SignedProposalPart { part: ProposalPart::Init(init), signature: RawSignature::default() }
    }

    /// Creates a signed fin part with default (empty) signature.
    pub fn fin(fin: ProposalFin) -> Self {
        SignedProposalPart { part: ProposalPart::Fin(fin), signature: RawSignature::default() }
    }

    /// Creates a signed transactions part with default (empty) signature.
    pub fn transactions(batch: TransactionBatch) -> Self {
        SignedProposalPart {
            part: ProposalPart::Transactions(batch),
            signature: RawSignature::default(),
        }
    }
}

impl TryInto<ProposalInit> for SignedProposalPart {
    type Error = ProtobufConversionError;

    fn try_into(self: SignedProposalPart) -> Result<ProposalInit, Self::Error> {
        match self.part {
            ProposalPart::Init(init) => Ok(init),
            _ => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "SignedProposalPart",
                expected: "Init",
                value_as_str: format!("{:?}", self.part),
            }),
        }
    }
}

impl From<ProposalInit> for ProposalPart {
    fn from(value: ProposalInit) -> Self {
        ProposalPart::Init(value)
    }
}

impl From<ProposalInit> for SignedProposalPart {
    fn from(value: ProposalInit) -> Self {
        SignedProposalPart::init(value)
    }
}

impl<T, StreamId> std::fmt::Display for StreamMessage<T, StreamId>
where
    T: Clone + IntoFromProto,
    StreamId: IntoFromProto + Clone + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let StreamMessageBody::Content(message) = &self.message {
            let message: Vec<u8> = message.clone().into();
            write!(
                f,
                "StreamMessage {{ stream_id: {}, message_id: {}, message_length: {}}}",
                self.stream_id,
                self.message_id,
                message.len(),
            )
        } else {
            write!(
                f,
                "StreamMessage {{ stream_id: {}, message_id: {}, message is fin }}",
                self.stream_id, self.message_id,
            )
        }
    }
}

/// HeighAndRound is a tuple struct used as the StreamId for consensus and context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeightAndRound(pub u64, pub u32);

impl TryFrom<Vec<u8>> for HeightAndRound {
    type Error = ProtobufConversionError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() != 12 {
            return Err(ProtobufConversionError::DecodeError(DecodeError::new("Invalid length")));
        }
        let mut bytes = value.as_slice();
        let height = bytes.get_u64();
        let round = bytes.get_u32();
        Ok(HeightAndRound(height, round))
    }
}

impl From<HeightAndRound> for Vec<u8> {
    fn from(value: HeightAndRound) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(12);
        bytes.put_u64(value.0);
        bytes.put_u32(value.1);
        bytes
    }
}

impl std::fmt::Display for HeightAndRound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(height={}, round={})", self.0, self.1)
    }
}
