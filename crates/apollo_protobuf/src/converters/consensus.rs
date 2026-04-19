#[cfg(test)]
#[path = "consensus_test.rs"]
mod consensus_test;

use std::convert::{TryFrom, TryInto};

use apollo_batcher_types::batcher_types::FinishedProposalInfo;
use prost::Message;
use starknet_api::block::{BlockNumber, GasPrice, StarknetVersion};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StarkHash;

use super::common::{
    enum_int_to_l1_data_availability_mode,
    l1_data_availability_mode_to_enum_int,
    missing,
};
use crate::consensus::{
    CommitmentParts,
    IntoFromProto,
    L2GasInfo,
    ProposalCommitment,
    ProposalFin,
    ProposalFinPayload,
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

impl TryFrom<protobuf::Hash> for ProposalCommitment {
    type Error = ProtobufConversionError;

    fn try_from(value: protobuf::Hash) -> Result<Self, Self::Error> {
        let stark_hash: StarkHash = value.try_into()?;
        Ok(ProposalCommitment(stark_hash))
    }
}

impl From<ProposalCommitment> for protobuf::Hash {
    fn from(value: ProposalCommitment) -> Self {
        value.0.into()
    }
}

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

        let height = BlockNumber(value.height);
        let round = value.round;
        let proposal_commitment: Option<ProposalCommitment> = value
            .proposal_commitment
            .map(|proposal_commitment| proposal_commitment.try_into())
            .transpose()?
            .map(ProposalCommitment);
        let voter = value.voter.ok_or(missing("voter"))?.try_into()?;
        // Convert Hashes to RawSignature (default to empty if None)
        let signature =
            value.signature.map(|hashes| hashes.try_into()).transpose()?.unwrap_or_default();

        Ok(Vote { vote_type, height, round, proposal_commitment, voter, signature })
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
            height: value.height.0,
            round: value.round,
            proposal_commitment: value.proposal_commitment.map(|commitment| commitment.0.into()),
            voter: Some(value.voter.into()),
            signature: Some(value.signature.into()),
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
        let height = BlockNumber(value.height);
        let round = value.round;
        let valid_round = value.valid_round;
        let proposer = value.proposer.ok_or(missing("proposer"))?.try_into()?;
        let timestamp = value.timestamp;
        let builder = value.builder.ok_or(missing("builder"))?.try_into()?;
        let l1_da_mode = enum_int_to_l1_data_availability_mode(value.l1_da_mode)?;
        let l2_gas_price_fri =
            GasPrice(value.l2_gas_price_fri.ok_or(missing("l2_gas_price_fri"))?.into());
        let l1_gas_price_fri =
            GasPrice(value.l1_gas_price_fri.ok_or(missing("l1_gas_price_fri"))?.into());
        let l1_data_gas_price_fri =
            GasPrice(value.l1_data_gas_price_fri.ok_or(missing("l1_data_gas_price_fri"))?.into());
        let l1_gas_price_wei =
            GasPrice(value.l1_gas_price_wei.ok_or(missing("l1_gas_price_wei"))?.into());
        let l1_data_gas_price_wei =
            GasPrice(value.l1_data_gas_price_wei.ok_or(missing("l1_data_gas_price_wei"))?.into());
        let starknet_version = match StarknetVersion::try_from(value.starknet_version.to_owned()) {
            Ok(version) => version,
            Err(_) => {
                return Err(ProtobufConversionError::OutOfRangeValue {
                    type_description: "starknet version",
                    value_as_str: value.starknet_version,
                });
            }
        };
        let version_constant_commitment = value
            .version_constant_commitment
            .ok_or(missing("version_constant_commitment"))?
            .try_into()?;
        let fee_proposal_fri = value.fee_proposal_fri.map(|raw| GasPrice(raw.into()));
        Ok(ProposalInit {
            height,
            round,
            valid_round,
            proposer,
            timestamp,
            builder,
            l1_da_mode,
            l2_gas_price_fri,
            l1_gas_price_fri,
            l1_data_gas_price_fri,
            l1_gas_price_wei,
            l1_data_gas_price_wei,
            starknet_version,
            version_constant_commitment,
            fee_proposal_fri,
        })
    }
}

impl From<ProposalInit> for protobuf::ProposalInit {
    fn from(value: ProposalInit) -> Self {
        protobuf::ProposalInit {
            height: value.height.0,
            round: value.round,
            valid_round: value.valid_round,
            proposer: Some(value.proposer.into()),
            timestamp: value.timestamp,
            builder: Some(value.builder.into()),
            l1_da_mode: l1_data_availability_mode_to_enum_int(value.l1_da_mode),
            l2_gas_price_fri: Some(value.l2_gas_price_fri.0.into()),
            l1_gas_price_fri: Some(value.l1_gas_price_fri.0.into()),
            l1_data_gas_price_fri: Some(value.l1_data_gas_price_fri.0.into()),
            l1_gas_price_wei: Some(value.l1_gas_price_wei.0.into()),
            l1_data_gas_price_wei: Some(value.l1_data_gas_price_wei.0.into()),
            starknet_version: value.starknet_version.to_string(),
            version_constant_commitment: Some(value.version_constant_commitment.into()),
            fee_proposal_fri: value.fee_proposal_fri.map(|gp| gp.0.into()),
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ProposalInit, protobuf::ProposalInit);

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

impl TryFrom<protobuf::CommitmentParts> for CommitmentParts {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::CommitmentParts) -> Result<Self, Self::Error> {
        let concatenated_counts = starknet_types_core::felt::Felt::try_from(
            value.concatenated_counts.ok_or(missing("concatenated_counts"))?,
        )?;
        let parent_commitment = value.parent_commitment.map(TryInto::try_into).transpose()?;
        let transaction_commitment =
            value.transaction_commitment.ok_or(missing("transaction_commitment"))?.try_into()?;
        let event_commitment =
            value.event_commitment.ok_or(missing("event_commitment"))?.try_into()?;
        let receipt_commitment =
            value.receipt_commitment.ok_or(missing("receipt_commitment"))?.try_into()?;
        Ok(CommitmentParts {
            concatenated_counts,
            parent_commitment,
            transaction_commitment,
            event_commitment,
            receipt_commitment,
        })
    }
}

impl From<CommitmentParts> for protobuf::CommitmentParts {
    fn from(value: CommitmentParts) -> Self {
        protobuf::CommitmentParts {
            concatenated_counts: Some(value.concatenated_counts.into()),
            parent_commitment: value.parent_commitment.map(Into::into),
            transaction_commitment: Some(value.transaction_commitment.into()),
            event_commitment: Some(value.event_commitment.into()),
            receipt_commitment: Some(value.receipt_commitment.into()),
        }
    }
}

impl From<&FinishedProposalInfo> for CommitmentParts {
    fn from(info: &FinishedProposalInfo) -> Self {
        let commitments = &info.block_header_commitments;
        CommitmentParts {
            concatenated_counts: commitments.concatenated_counts,
            parent_commitment: info
                .parent_proposal_commitment
                .as_ref()
                .map(|p| ProposalCommitment(p.partial_block_hash.0)),
            transaction_commitment: commitments.transaction_commitment.0,
            event_commitment: commitments.event_commitment.0,
            receipt_commitment: commitments.receipt_commitment.0,
        }
    }
}

impl TryFrom<protobuf::L2GasInfo> for L2GasInfo {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::L2GasInfo) -> Result<Self, Self::Error> {
        let next_l2_gas_price_fri =
            value.next_l2_gas_price_fri.ok_or(missing("next_l2_gas_price_fri"))?.into();
        let l2_gas_used = GasAmount(value.l2_gas_used);
        Ok(L2GasInfo { next_l2_gas_price_fri: GasPrice(next_l2_gas_price_fri), l2_gas_used })
    }
}

impl From<L2GasInfo> for protobuf::L2GasInfo {
    fn from(value: L2GasInfo) -> Self {
        protobuf::L2GasInfo {
            next_l2_gas_price_fri: Some(value.next_l2_gas_price_fri.0.into()),
            l2_gas_used: value.l2_gas_used.0,
        }
    }
}

impl TryFrom<protobuf::ProposalFinPayload> for ProposalFinPayload {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ProposalFinPayload) -> Result<Self, Self::Error> {
        let commitment_parts =
            value.commitment_parts.ok_or(missing("commitment_parts"))?.try_into()?;
        let l2_gas_info = value.l2_gas_info.ok_or(missing("l2_gas_info"))?.try_into()?;
        Ok(ProposalFinPayload { commitment_parts, l2_gas_info })
    }
}

impl From<ProposalFinPayload> for protobuf::ProposalFinPayload {
    fn from(value: ProposalFinPayload) -> Self {
        protobuf::ProposalFinPayload {
            commitment_parts: Some(value.commitment_parts.into()),
            l2_gas_info: Some(value.l2_gas_info.into()),
        }
    }
}

impl TryFrom<protobuf::ProposalFin> for ProposalFin {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ProposalFin) -> Result<Self, Self::Error> {
        let proposal_commitment: ProposalCommitment =
            value.proposal_commitment.ok_or(missing("proposal_commitment"))?.try_into()?;
        let executed_transaction_count = value.executed_transaction_count;
        let fin_payload = value.fin_payload.map(TryInto::try_into).transpose()?;
        Ok(ProposalFin { proposal_commitment, executed_transaction_count, fin_payload })
    }
}

impl From<ProposalFin> for protobuf::ProposalFin {
    fn from(value: ProposalFin) -> Self {
        protobuf::ProposalFin {
            proposal_commitment: Some(value.proposal_commitment.into()),
            executed_transaction_count: value.executed_transaction_count,
            fin_payload: value.fin_payload.map(Into::into),
        }
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
            Message::Transactions(content) => Ok(ProposalPart::Transactions(content.try_into()?)),
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
            ProposalPart::Transactions(content) => protobuf::ProposalPart {
                message: Some(protobuf::proposal_part::Message::Transactions(content.into())),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(ProposalPart, protobuf::ProposalPart);
