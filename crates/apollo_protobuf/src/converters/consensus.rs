#[cfg(test)]
#[path = "consensus_test.rs"]
mod consensus_test;

use std::convert::{TryFrom, TryInto};

use prost::Message;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::hash::StarkHash;

use super::common::{
    enum_int_to_l1_data_availability_mode,
    l1_data_availability_mode_to_enum_int,
    missing,
};
use crate::consensus::{
    ConsensusBlockInfo,
    IntoFromProto,
    ProposalCommitment,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    StreamMessageBody,
    TransactionBatch,
    Vote,
    VoteType,
};
use crate::converters::ProtobufConversionError;
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

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
        let proposal_commitment: Option<ProposalCommitment> = value
            .proposal_commitment
            .map(|proposal_commitment| proposal_commitment.try_into())
            .transpose()?
            .map(ProposalCommitment);
        let voter = value.voter.ok_or(missing("voter"))?.try_into()?;

        Ok(Vote { vote_type, height, round, proposal_commitment, voter })
    }
}

impl From<Vote> for protobuf::Vote {
    fn from(value: Vote) -> Self {
        let vote_type = match value.vote_type {
            VoteType::Prevote => protobuf::vote::VoteType::Prevote,
            VoteType::Precommit => protobuf::vote::VoteType::Precommit,
        };

        protobuf::Vote {
            vote_type: i32::from(vote_type),
            height: value.height,
            round: value.round,
            proposal_commitment: value.proposal_commitment.map(|commitment| commitment.0.into()),
            voter: Some(value.voter.into()),
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(Vote, protobuf::Vote);

impl<T, StreamId> TryFrom<protobuf::StreamMessage> for StreamMessage<T, StreamId>
where
    T: IntoFromProto,
    StreamId: IntoFromProto + Clone,
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
            stream_id: value.stream_id.try_into()?,
            message_id: value.message_id,
        })
    }
}

impl<T, StreamId> From<StreamMessage<T, StreamId>> for protobuf::StreamMessage
where
    T: IntoFromProto,
    StreamId: IntoFromProto + Clone,
{
    fn from(value: StreamMessage<T, StreamId>) -> Self {
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
            stream_id: value.stream_id.into(),
            message_id: value.message_id,
        }
    }
}

// Can't use auto_impl_into_and_try_from_vec_u8!(StreamMessage, protobuf::StreamMessage);
// because it doesn't seem to work with generics.
// TODO(guyn): consider expanding the macro to support generics
impl<T, StreamId> From<StreamMessage<T, StreamId>> for Vec<u8>
where
    T: IntoFromProto,
    StreamId: IntoFromProto + Clone,
{
    fn from(value: StreamMessage<T, StreamId>) -> Self {
        let protobuf_value = <protobuf::StreamMessage>::from(value);
        protobuf_value.encode_to_vec()
    }
}

impl<T, StreamId> TryFrom<Vec<u8>> for StreamMessage<T, StreamId>
where
    T: IntoFromProto,
    StreamId: IntoFromProto + Clone,
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

impl TryFrom<protobuf::ProposalInit> for ProposalInit {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ProposalInit) -> Result<Self, Self::Error> {
        let height = value.height;
        let round = value.round;
        let valid_round = value.valid_round;
        let proposer = value.proposer.ok_or(missing("proposer"))?.try_into()?;
        Ok(ProposalInit { height: BlockNumber(height), round, valid_round, proposer })
    }
}

impl From<ProposalInit> for protobuf::ProposalInit {
    fn from(value: ProposalInit) -> Self {
        protobuf::ProposalInit {
            height: value.height.0,
            round: value.round,
            valid_round: value.valid_round,
            proposer: Some(value.proposer.into()),
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ProposalInit, protobuf::ProposalInit);

impl TryFrom<protobuf::BlockInfo> for ConsensusBlockInfo {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockInfo) -> Result<Self, Self::Error> {
        let height = value.height;
        let timestamp = value.timestamp;
        let builder = value.builder.ok_or(missing("builder"))?.try_into()?;
        let l1_da_mode = enum_int_to_l1_data_availability_mode(value.l1_da_mode)?;
        let l2_gas_price_fri =
            GasPrice(value.l2_gas_price_fri.ok_or(missing("l2_gas_price_fri"))?.into());
        let l1_gas_price_wei =
            GasPrice(value.l1_gas_price_wei.ok_or(missing("l1_gas_price_wei"))?.into());
        let l1_data_gas_price_wei =
            GasPrice(value.l1_data_gas_price_wei.ok_or(missing("l1_data_gas_price_wei"))?.into());
        let eth_to_fri_rate = value.eth_to_fri_rate.ok_or(missing("eth_to_fri_rate"))?.into();
        Ok(ConsensusBlockInfo {
            height: BlockNumber(height),
            timestamp,
            builder,
            l1_da_mode,
            l2_gas_price_fri,
            l1_gas_price_wei,
            l1_data_gas_price_wei,
            eth_to_fri_rate,
        })
    }
}

impl From<ConsensusBlockInfo> for protobuf::BlockInfo {
    fn from(value: ConsensusBlockInfo) -> Self {
        protobuf::BlockInfo {
            height: value.height.0,
            timestamp: value.timestamp,
            builder: Some(value.builder.into()),
            l1_da_mode: l1_data_availability_mode_to_enum_int(value.l1_da_mode),
            l1_gas_price_wei: Some(value.l1_gas_price_wei.0.into()),
            l1_data_gas_price_wei: Some(value.l1_data_gas_price_wei.0.into()),
            l2_gas_price_fri: Some(value.l2_gas_price_fri.0.into()),
            eth_to_fri_rate: Some(value.eth_to_fri_rate.into()),
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ConsensusBlockInfo, protobuf::BlockInfo);

impl TryFrom<protobuf::TransactionBatch> for TransactionBatch {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::TransactionBatch) -> Result<Self, Self::Error> {
        let transactions = value
            .transactions
            .into_iter()
            .map(|tx| tx.try_into())
            .collect::<Result<Vec<ConsensusTransaction>, ProtobufConversionError>>()?;
        Ok(TransactionBatch { transactions })
    }
}

impl From<TransactionBatch> for protobuf::TransactionBatch {
    fn from(value: TransactionBatch) -> Self {
        let transactions = value.transactions.into_iter().map(Into::into).collect();
        protobuf::TransactionBatch { transactions }
    }
}

auto_impl_into_and_try_from_vec_u8!(TransactionBatch, protobuf::TransactionBatch);

impl TryFrom<protobuf::ProposalFin> for ProposalFin {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ProposalFin) -> Result<Self, Self::Error> {
        let proposal_commitment: StarkHash =
            value.proposal_commitment.ok_or(missing("proposal_commitment"))?.try_into()?;
        let proposal_commitment = ProposalCommitment(proposal_commitment);
        Ok(ProposalFin { proposal_commitment })
    }
}

impl From<ProposalFin> for protobuf::ProposalFin {
    fn from(value: ProposalFin) -> Self {
        protobuf::ProposalFin { proposal_commitment: Some(value.proposal_commitment.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(ProposalFin, protobuf::ProposalFin);

impl TryFrom<protobuf::ProposalPart> for ProposalPart {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ProposalPart) -> Result<Self, Self::Error> {
        use protobuf::proposal_part::Message;

        let Some(part) = value.message else {
            return Err(missing("part"));
        };

        match part {
            Message::Init(init) => Ok(ProposalPart::Init(init.try_into()?)),
            Message::Fin(fin) => Ok(ProposalPart::Fin(fin.try_into()?)),
            Message::BlockInfo(block_info) => Ok(ProposalPart::BlockInfo(block_info.try_into()?)),
            Message::Transactions(content) => Ok(ProposalPart::Transactions(content.try_into()?)),
            Message::ExecutedTransactionCount(count) => {
                Ok(ProposalPart::ExecutedTransactionCount(count))
            }
        }
    }
}

impl From<ProposalPart> for protobuf::ProposalPart {
    fn from(value: ProposalPart) -> Self {
        match value {
            ProposalPart::Init(init) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::Init(init.into())),
            },
            ProposalPart::Fin(fin) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::Fin(fin.into())),
            },
            ProposalPart::BlockInfo(block_info) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::BlockInfo(block_info.into())),
            },
            ProposalPart::Transactions(content) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::Transactions(content.into())),
            },
            ProposalPart::ExecutedTransactionCount(count) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::ExecutedTransactionCount(count)),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ProposalPart, protobuf::ProposalPart);
