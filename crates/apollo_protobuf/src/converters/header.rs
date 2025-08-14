#[cfg(test)]
#[path = "header_test.rs"]
mod header_test;

use prost::Message;
use starknet_api::block::{
    BlockHash,
    BlockHeader,
    BlockHeaderWithoutHash,
    BlockNumber,
    BlockSignature,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::crypto::utils::Signature;
use starknet_api::hash::PoseidonHash;

use super::common::{
    enum_int_to_l1_data_availability_mode,
    l1_data_availability_mode_to_enum_int,
    missing,
};
use super::ProtobufConversionError;
use crate::sync::{DataOrFin, HeaderQuery, Query, SignedBlockHeader};
use crate::{auto_impl_into_and_try_from_vec_u8, protobuf};

impl TryFrom<protobuf::BlockHeadersResponse> for DataOrFin<SignedBlockHeader> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersResponse) -> Result<Self, Self::Error> {
        Ok(Self(value.try_into()?))
    }
}

impl TryFrom<protobuf::BlockHeadersResponse> for Option<SignedBlockHeader> {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersResponse) -> Result<Self, Self::Error> {
        match value.header_message {
            Some(protobuf::block_headers_response::HeaderMessage::Header(header)) => {
                Ok(Some(header.try_into()?))
            }
            Some(protobuf::block_headers_response::HeaderMessage::Fin(_)) => Ok(None),
            None => Err(missing("BlockHeadersResponse::header_message")),
        }
    }
}

impl TryFrom<protobuf::SignedBlockHeader> for SignedBlockHeader {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::SignedBlockHeader) -> Result<Self, Self::Error> {
        let block_hash = value
            .block_hash
            .ok_or(missing("SignedBlockHeader::block_hash"))?
            .try_into()
            .map(BlockHash)?;

        let parent_hash = value
            .parent_hash
            .ok_or(missing("SignedBlockHeader::parent_hash"))?
            .try_into()
            .map(BlockHash)?;

        let timestamp = starknet_api::block::BlockTimestamp(value.time);

        let sequencer = value
            .sequencer_address
            .ok_or(missing("SignedBlockHeader::sequencer_address"))?
            .try_into()
            .map(SequencerContractAddress)?;

        let state_root = value
            .state_root
            .ok_or(missing("SignedBlockHeader::state_root"))?
            .try_into()
            .map(GlobalRoot)?;

        let n_transactions = value
            .transactions
            .as_ref()
            .ok_or(missing("SignedBlockHeader::transactions"))?
            .n_leaves
            .try_into()
            .expect("Failed converting u64 to usize");

        let transaction_commitment = value
            .transactions
            .ok_or(missing("SignedBlockHeader::transactions"))?
            .root
            .map(|root| root.try_into())
            .transpose()?
            .map(TransactionCommitment);

        let n_events = value
            .events
            .as_ref()
            .ok_or(missing("SignedBlockHeader::events"))?
            .n_leaves
            .try_into()
            .expect("Failed converting u64 to usize");

        let event_commitment = value
            .events
            .ok_or(missing("SignedBlockHeader::events"))?
            .root
            .map(|root| root.try_into())
            .transpose()?
            .map(EventCommitment);

        let state_diff_length = value.state_diff_commitment.as_ref().map(|state_diff_commitment| {
            state_diff_commitment
                .state_diff_length
                .try_into()
                .expect("Failed converting u64 to usize")
        });

        let l1_da_mode = enum_int_to_l1_data_availability_mode(value.l1_data_availability_mode)?;

        let starknet_version = match StarknetVersion::try_from(value.protocol_version.to_owned()) {
            Ok(version) => Some(version),
            Err(_) => {
                return Err(ProtobufConversionError::OutOfRangeValue {
                    type_description: "starknet version",
                    value_as_str: value.protocol_version,
                });
            }
        };

        let l1_gas_price = GasPricePerToken {
            price_in_fri: u128::from(
                value.l1_gas_price_fri.ok_or(missing("SignedBlockHeader::gas_price_fri"))?,
            )
            .into(),

            price_in_wei: u128::from(
                value.l1_gas_price_wei.ok_or(missing("SignedBlockHeader::gas_price_wei"))?,
            )
            .into(),
        };

        let l1_data_gas_price = GasPricePerToken {
            price_in_fri: u128::from(
                value
                    .l1_data_gas_price_fri
                    .ok_or(missing("SignedBlockHeader::data_gas_price_fri"))?,
            )
            .into(),
            price_in_wei: u128::from(
                value
                    .l1_data_gas_price_wei
                    .ok_or(missing("SignedBlockHeader::data_gas_price_wei"))?,
            )
            .into(),
        };
        let l2_gas_price = GasPricePerToken {
            price_in_fri: u128::from(
                value.l2_gas_price_fri.ok_or(missing("SignedBlockHeader::l2_gas_price_fri"))?,
            )
            .into(),

            price_in_wei: u128::from(
                value.l2_gas_price_wei.ok_or(missing("SignedBlockHeader::l2_gas_price_wei"))?,
            )
            .into(),
        };

        let l2_gas_consumed = value.l2_gas_consumed.into();
        let next_l2_gas_price = u128::from(
            value.next_l2_gas_price.ok_or(missing("SignedBlockHeader::next_l2_gas_price"))?,
        )
        .into();

        let receipt_commitment = value
            .receipts
            .map(|receipts| receipts.try_into().map(ReceiptCommitment))
            .transpose()?;

        let state_diff_commitment = value
            .state_diff_commitment
            .ok_or(missing("SignedBlockHeader::state_diff_commitment"))?
            .root
            .map(|root| root.try_into())
            .transpose()?
            .map(|hash| StateDiffCommitment(PoseidonHash(hash)));

        Ok(SignedBlockHeader {
            block_header: BlockHeader {
                block_hash,
                block_header_without_hash: BlockHeaderWithoutHash {
                    parent_hash,
                    block_number: BlockNumber(value.number),
                    l1_gas_price,
                    l1_data_gas_price,
                    l2_gas_price,
                    l2_gas_consumed,
                    next_l2_gas_price,
                    state_root,
                    sequencer,
                    timestamp,
                    l1_da_mode,
                    starknet_version,
                },
                state_diff_commitment,
                state_diff_length,
                transaction_commitment,
                event_commitment,
                n_transactions,
                n_events,
                receipt_commitment,
            },
            // collect will convert from Vec<Result> to Result<Vec>.
            signatures: value
                .signatures
                .into_iter()
                .map(starknet_api::block::BlockSignature::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl From<DataOrFin<SignedBlockHeader>> for protobuf::BlockHeadersResponse {
    fn from(value: DataOrFin<SignedBlockHeader>) -> Self {
        value.0.into()
    }
}

impl From<(BlockHeader, Vec<BlockSignature>)> for protobuf::SignedBlockHeader {
    fn from((header, signatures): (BlockHeader, Vec<BlockSignature>)) -> Self {
        let state_diff_commitment =
            header.state_diff_length.map(|state_diff_length| protobuf::StateDiffCommitment {
                state_diff_length: state_diff_length
                    .try_into()
                    .expect("Converting usize to u64 failed"),
                root: header
                    .state_diff_commitment
                    .map(|state_diff_commitment| state_diff_commitment.0.0.into()),
            });
        Self {
            block_hash: Some(header.block_hash.into()),
            parent_hash: Some(header.block_header_without_hash.parent_hash.into()),
            number: header.block_header_without_hash.block_number.0,
            time: header.block_header_without_hash.timestamp.0,
            sequencer_address: Some(header.block_header_without_hash.sequencer.0.into()),
            state_diff_commitment,
            state_root: Some(header.block_header_without_hash.state_root.0.into()),
            transactions: Some(protobuf::Patricia {
                n_leaves: header.n_transactions.try_into().expect("Converting usize to u64 failed"),
                root: header
                    .transaction_commitment
                    .map(|transaction_commitment| transaction_commitment.0.into()),
            }),
            events: Some(protobuf::Patricia {
                n_leaves: header.n_events.try_into().expect("Converting usize to u64 failed"),
                root: header.event_commitment.map(|event_commitment| event_commitment.0.into()),
            }),
            receipts: header
                .receipt_commitment
                .map(|receipt_commitment| receipt_commitment.0.into()),
            protocol_version: header
                .block_header_without_hash
                .starknet_version
                .unwrap_or_default()
                .to_string(),
            l1_gas_price_wei: Some(
                header.block_header_without_hash.l1_gas_price.price_in_wei.0.into(),
            ),
            l1_gas_price_fri: Some(
                header.block_header_without_hash.l1_gas_price.price_in_fri.0.into(),
            ),
            l1_data_gas_price_wei: Some(
                header.block_header_without_hash.l1_data_gas_price.price_in_wei.0.into(),
            ),
            l1_data_gas_price_fri: Some(
                header.block_header_without_hash.l1_data_gas_price.price_in_fri.0.into(),
            ),
            l2_gas_price_wei: Some(
                header.block_header_without_hash.l2_gas_price.price_in_wei.0.into(),
            ),
            l2_gas_price_fri: Some(
                header.block_header_without_hash.l2_gas_price.price_in_fri.0.into(),
            ),
            l1_data_availability_mode: l1_data_availability_mode_to_enum_int(
                header.block_header_without_hash.l1_da_mode,
            ),
            l2_gas_consumed: header.block_header_without_hash.l2_gas_consumed.0,
            next_l2_gas_price: Some(header.block_header_without_hash.next_l2_gas_price.0.into()),
            signatures: signatures.iter().map(|signature| (*signature).into()).collect(),
        }
    }
}

impl TryFrom<protobuf::ConsensusSignature> for starknet_api::block::BlockSignature {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::ConsensusSignature) -> Result<Self, Self::Error> {
        Ok(Self(Signature {
            r: value.r.ok_or(missing("SignedBlockHeader::r"))?.try_into()?,
            s: value.s.ok_or(missing("SignedBlockHeader::s"))?.try_into()?,
        }))
    }
}

impl From<starknet_api::block::BlockSignature> for protobuf::ConsensusSignature {
    fn from(value: starknet_api::block::BlockSignature) -> Self {
        Self { r: Some(value.0.r.into()), s: Some(value.0.s.into()) }
    }
}

impl From<Option<SignedBlockHeader>> for protobuf::BlockHeadersResponse {
    fn from(data: Option<SignedBlockHeader>) -> Self {
        match data {
            Some(SignedBlockHeader { block_header, signatures }) => {
                protobuf::BlockHeadersResponse {
                    header_message: Some(protobuf::block_headers_response::HeaderMessage::Header(
                        (block_header, signatures).into(),
                    )),
                }
            }
            None => protobuf::BlockHeadersResponse {
                header_message: Some(protobuf::block_headers_response::HeaderMessage::Fin(
                    protobuf::Fin {},
                )),
            },
        }
    }
}

auto_impl_into_and_try_from_vec_u8!(DataOrFin<SignedBlockHeader>, protobuf::BlockHeadersResponse);

// TODO(shahak): Erase this once network stops using it.
impl TryFrom<protobuf::BlockHeadersRequest> for Query {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersRequest) -> Result<Self, Self::Error> {
        Ok(HeaderQuery::try_from(value)?.0)
    }
}

impl TryFrom<protobuf::BlockHeadersRequest> for HeaderQuery {
    type Error = ProtobufConversionError;
    fn try_from(value: protobuf::BlockHeadersRequest) -> Result<Self, Self::Error> {
        Ok(HeaderQuery(
            value.iteration.ok_or(missing("BlockHeadersRequest::iteration"))?.try_into()?,
        ))
    }
}

// TODO(shahak): Erase this once network stops using it.
impl From<Query> for protobuf::BlockHeadersRequest {
    fn from(value: Query) -> Self {
        protobuf::BlockHeadersRequest { iteration: Some(value.into()) }
    }
}

impl From<HeaderQuery> for protobuf::BlockHeadersRequest {
    fn from(value: HeaderQuery) -> Self {
        protobuf::BlockHeadersRequest { iteration: Some(value.0.into()) }
    }
}

auto_impl_into_and_try_from_vec_u8!(HeaderQuery, protobuf::BlockHeadersRequest);
