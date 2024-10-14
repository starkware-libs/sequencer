#[cfg(test)]
#[path = "consensus_test.rs"]
mod consensus_test;
use std::convert::{TryFrom, TryInto};

use prost::Message;
use starknet_api::block::BlockHash;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::Transaction;

use crate::consensus::{
    ConsensusMessage,
    Proposal,
    StreamMessage,
    StreamMessageBody,
    Vote,
    VoteType,
};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::Proposal> for Proposal {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Proposal) -> Result<Self, Self::Error> {
        let transactions = value
            .transactions
            .into_iter()
            .map(|tx| tx.try_into())
            .collect::<Result<Vec<Transaction>, ProtobufConversionError>>()?;

        let height = value.height;
        let round = value.round;
        let proposer = value
            .proposer
            .ok_or(ProtobufConversionError::MissingField { field_description: "proposer" })?
            .try_into()?;
        let block_hash: StarkHash = value
            .block_hash
            .ok_or(ProtobufConversionError::MissingField { field_description: "block_hash" })?
            .try_into()?;
        let block_hash = BlockHash(block_hash);
        let valid_round = value.valid_round;

        Ok(Proposal { height, round, proposer, transactions, block_hash, valid_round })
    }
}

impl From<Proposal> for protobuf::Proposal {
    fn from(value: Proposal) -> Self {
        let transactions = value.transactions.into_iter().map(Into::into).collect();

        protobuf::Proposal {
            height: value.height,
            round: value.round,
            proposer: Some(value.proposer.into()),
            transactions,
            block_hash: Some(value.block_hash.0.into()),
            valid_round: value.valid_round,
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(Proposal, protobuf::Proposal);

impl TryFrom<protobuf::vote::VoteType> for VoteType {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::vote::VoteType) -> Result<Self, Self::Error> {
        match value {
            protobuf::vote::VoteType::Prevote => Ok(VoteType::Prevote),
            protobuf::vote::VoteType::Precommit => Ok(VoteType::Precommit),
        }
    }
}

impl From<VoteType> for protobuf::vote::VoteType {
    fn from(value: VoteType) -> Self {
        match value {
            VoteType::Prevote => protobuf::vote::VoteType::Prevote,
            VoteType::Precommit => protobuf::vote::VoteType::Precommit,
        }
    }
}

impl TryFrom<protobuf::Vote> for Vote {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Vote) -> Result<Self, Self::Error> {
        let vote_type = protobuf::vote::VoteType::try_from(value.vote_type)?.try_into()?;

        let height = value.height;
        let round = value.round;
        let block_hash: Option<BlockHash> =
            value.block_hash.map(|block_hash| block_hash.try_into()).transpose()?.map(BlockHash);
        let voter = value
            .voter
            .ok_or(ProtobufConversionError::MissingField { field_description: "voter" })?
            .try_into()?;

        Ok(Vote { vote_type, height, round, block_hash, voter })
    }
}

impl From<Vote> for protobuf::Vote {
    fn from(value: Vote) -> Self {
        let vote_type = match value.vote_type {
            VoteType::Prevote => protobuf::vote::VoteType::Prevote,
            VoteType::Precommit => protobuf::vote::VoteType::Precommit,
        };

        protobuf::Vote {
            vote_type: vote_type as i32,
            height: value.height,
            round: value.round,
            block_hash: value.block_hash.map(|hash| hash.0.into()),
            voter: Some(value.voter.into()),
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(Vote, protobuf::Vote);

impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>>
    TryFrom<protobuf::StreamMessage> for StreamMessage<T>
{
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::StreamMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            message: match value {
                protobuf::StreamMessage {
                    message: Some(protobuf::stream_message::Message::Content(message)),
                    stream_id: _,
                    message_id: _,
                } => StreamMessageBody::Content(message.try_into()?),
                protobuf::StreamMessage {
                    message: Some(protobuf::stream_message::Message::Fin(protobuf::Fin {})),
                    stream_id: _,
                    message_id: _,
                } => StreamMessageBody::Fin,
                protobuf::StreamMessage { message: None, stream_id: _, message_id: _ } => {
                    StreamMessageBody::Fin
                }
            },
            stream_id: value.stream_id,
            message_id: value.message_id,
        })
    }
}

impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> From<StreamMessage<T>>
    for protobuf::StreamMessage
{
    fn from(value: StreamMessage<T>) -> Self {
        Self {
            message: match value {
                StreamMessage {
                    message: StreamMessageBody::Content(message),
                    stream_id: _,
                    message_id: _,
                } => Some(protobuf::stream_message::Message::Content(message.into())),
                StreamMessage { message: StreamMessageBody::Fin, stream_id: _, message_id: _ } => {
                    Some(protobuf::stream_message::Message::Fin(protobuf::Fin {}))
                }
            },
            stream_id: value.stream_id,
            message_id: value.message_id,
        }
    }
}

// Can't use auto_impl_into_and_try_from_vec_u8!(StreamMessage, protobuf::StreamMessage);
// because it doesn't seem to work with generics.
// TODO(guyn): consider expanding the macro to support generics
impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> From<StreamMessage<T>>
    for Vec<u8>
{
    fn from(value: StreamMessage<T>) -> Self {
        let protobuf_value = <protobuf::StreamMessage>::from(value);
        protobuf_value.encode_to_vec()
    }
}

impl<T: Into<Vec<u8>> + TryFrom<Vec<u8>, Error = ProtobufConversionError>> TryFrom<Vec<u8>>
    for StreamMessage<T>
{
    type Error = ProtobufConversionError;
    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        let protobuf_value = <protobuf::StreamMessage>::decode(&value[..])?;
        match Self::try_from(protobuf_value) {
            Ok(value) => Ok(value),
            Err(e) => Err(e),
        }
    }
}

impl TryFrom<protobuf::ConsensusMessage> for ConsensusMessage {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::ConsensusMessage) -> Result<Self, Self::Error> {
        use protobuf::consensus_message::Message;

        let Some(message) = value.message else {
            return Err(ProtobufConversionError::MissingField { field_description: "message" });
        };

        match message {
            Message::Proposal(proposal) => Ok(ConsensusMessage::Proposal(proposal.try_into()?)),
            Message::Vote(vote) => Ok(ConsensusMessage::Vote(vote.try_into()?)),
        }
    }
}

impl From<ConsensusMessage> for protobuf::ConsensusMessage {
    fn from(value: ConsensusMessage) -> Self {
        match value {
            ConsensusMessage::Proposal(proposal) => protobuf::ConsensusMessage {
                message: Some(protobuf::consensus_message::Message::Proposal(proposal.into())),
            },
            ConsensusMessage::Vote(vote) => protobuf::ConsensusMessage {
                message: Some(protobuf::consensus_message::Message::Vote(vote.into())),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ConsensusMessage, protobuf::ConsensusMessage);
