#[cfg(test)]
#[path = "consensus_test.rs"]
mod consensus_test;

use std::fmt::Display;

use bytes::{Buf, BufMut};
use prost::DecodeError;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;

use crate::converters::ProtobufConversionError;

pub trait IntoFromProto: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError> {}
impl<T> IntoFromProto for T where
    T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>
{
}

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum VoteType {
    Prevote,
    #[default]
    Precommit,
}

#[derive(Debug, Default, Hash, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Vote {
    pub vote_type: VoteType,
    pub height: u64,
    pub round: u32,
    pub block_hash: Option<BlockHash>,
    pub voter: ContractAddress,
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

/// This message must be sent first when proposing a new block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProposalInit {
    /// The height of the consensus (block number).
    pub height: BlockNumber,
    /// The current round of the consensus.
    pub round: u32,
    /// The last round that was valid.
    pub valid_round: Option<u32>,
    /// Address of the one who proposed the block.
    pub proposer: ContractAddress,
}

/// This struct differs from `BlockInfo` in `starknet_api` because we send L1 gas prices in ETH and
/// include the ETH to STRK conversion rate. This allows for more informative validations, as we can
/// distinguish whether an issue comes from the L1 price reading or the conversion rate instead of
/// comparing after multiplication.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsensusBlockInfo {
    pub height: BlockNumber,
    pub timestamp: u64,
    pub builder: ContractAddress,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub l2_gas_price_fri: GasPrice,
    pub l1_gas_price_wei: GasPrice,
    pub l1_data_gas_price_wei: GasPrice,
    /// The value of 1 ETH in FRI.
    pub eth_to_fri_rate: u128,
}

/// A temporary constant to use as a validator ID. Zero is not a valid contract address.
// TODO(Matan): Remove this once we have a proper validator set.
pub const DEFAULT_VALIDATOR_ID: u64 = 100;

impl Default for ProposalInit {
    fn default() -> Self {
        ProposalInit {
            height: Default::default(),
            round: Default::default(),
            valid_round: Default::default(),
            proposer: ContractAddress::from(DEFAULT_VALIDATOR_ID),
        }
    }
}

/// There is one or more batches of transactions in a proposed block.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionBatch {
    /// The transactions in the batch.
    pub transactions: Vec<ConsensusTransaction>,
}

/// The proposal is done when receiving this fin message, which contains the block hash.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposalFin {
    /// The block hash of the proposed block.
    /// TODO(Matan): Consider changing the content ID to a signature.
    pub proposal_commitment: BlockHash,
}

/// A part of the proposal.
#[derive(Debug, Clone, PartialEq)]
pub enum ProposalPart {
    /// The initialization part of the proposal.
    Init(ProposalInit),
    /// Identifies the content of the proposal; contains `id(v)` in Tendermint terms.
    Fin(ProposalFin),
    /// The block info part of the proposal.
    BlockInfo(ConsensusBlockInfo),
    /// A part of the proposal that contains one or more transactions.
    Transactions(TransactionBatch),
    /// Number of executed transactions in the proposal.
    ExecutedTransactionCount(u64),
}

impl TryInto<ProposalInit> for ProposalPart {
    type Error = ProtobufConversionError;

    fn try_into(self: ProposalPart) -> Result<ProposalInit, Self::Error> {
        match self {
            ProposalPart::Init(init) => Ok(init),
            _ => Err(ProtobufConversionError::WrongEnumVariant {
                type_description: "ProposalPart",
                expected: "Init",
                value_as_str: format!("{:?}", self),
            }),
        }
    }
}

impl From<ProposalInit> for ProposalPart {
    fn from(value: ProposalInit) -> Self {
        ProposalPart::Init(value)
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
