use apollo_storage::db::TransactionKind;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::{StorageError, StorageReader, StorageTxn};
use jsonrpsee::types::ErrorObjectOwned;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockStatus, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{GlobalRoot, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;

use super::error::BLOCK_NOT_FOUND;
use super::transaction::Transactions;
use crate::api::{BlockHashOrNumber, BlockId, Tag};
use crate::{get_latest_block_number, internal_server_error};

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub sequencer_address: SequencerContractAddress,
    pub new_root: GlobalRoot,
    pub timestamp: BlockTimestamp,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub starknet_version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct PendingBlockHeader {
    pub parent_hash: BlockHash,
    pub sequencer_address: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub starknet_version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum GeneralBlockHeader {
    BlockHeader(BlockHeader),
    PendingBlockHeader(PendingBlockHeader),
}

impl From<starknet_api::block::BlockHeader> for BlockHeader {
    fn from(header: starknet_api::block::BlockHeader) -> Self {
        BlockHeader {
            block_hash: header.block_hash,
            parent_hash: header.block_header_without_hash.parent_hash,
            block_number: header.block_header_without_hash.block_number,
            sequencer_address: header.block_header_without_hash.sequencer,
            new_root: header.block_header_without_hash.state_root,
            timestamp: header.block_header_without_hash.timestamp,
            l1_gas_price: GasPricePerToken {
                price_in_wei: header.block_header_without_hash.l1_gas_price.price_in_wei,
                price_in_fri: header.block_header_without_hash.l1_gas_price.price_in_fri,
            },
            l1_data_gas_price: GasPricePerToken {
                price_in_wei: header.block_header_without_hash.l1_data_gas_price.price_in_wei,
                price_in_fri: header.block_header_without_hash.l1_data_gas_price.price_in_fri,
            },
            l2_gas_price: GasPricePerToken {
                price_in_wei: header.block_header_without_hash.l2_gas_price.price_in_wei,
                price_in_fri: header.block_header_without_hash.l2_gas_price.price_in_fri,
            },
            l1_da_mode: header.block_header_without_hash.l1_da_mode,
            starknet_version: match header.block_header_without_hash.starknet_version {
                Some(version) => version.to_string(),
                None => "missing".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Block {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BlockStatus>,
    #[serde(flatten)]
    pub header: GeneralBlockHeader,
    pub transactions: Transactions,
}

pub fn get_block_header_by_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_number: BlockNumber,
) -> Result<starknet_api::block::BlockHeader, ErrorObjectOwned> {
    let header = txn
        .get_block_header(block_number)
        .map_err(internal_server_error)?
        .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

    Ok(header)
}

/// Return the closest block number that corresponds to the given block id and is accepted (i.e not
/// pending). Latest block means the most advanced block that we've downloaded and that we've
/// downloaded its state diff.
pub(crate) fn get_accepted_block_number<Mode: TransactionKind>(
    txn: &StorageTxn<'_, Mode>,
    block_id: BlockId,
) -> Result<BlockNumber, ErrorObjectOwned> {
    Ok(match block_id {
        BlockId::HashOrNumber(BlockHashOrNumber::Hash(block_hash)) => {
            let block_number = txn
                .get_block_number_by_hash(&block_hash)
                .map_err(internal_server_error)?
                .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;

            // Check that the block has state diff.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;
            if block_number > last_block_number {
                return Err(ErrorObjectOwned::from(BLOCK_NOT_FOUND));
            }
            block_number
        }
        BlockId::HashOrNumber(BlockHashOrNumber::Number(block_number)) => {
            // Check that the block exists and has state diff.
            let last_block_number = get_latest_block_number(txn)?
                .ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?;
            if block_number > last_block_number {
                return Err(ErrorObjectOwned::from(BLOCK_NOT_FOUND));
            }
            block_number
        }
        BlockId::Tag(Tag::Latest | Tag::Pending) => {
            get_latest_block_number(txn)?.ok_or_else(|| ErrorObjectOwned::from(BLOCK_NOT_FOUND))?
        }
    })
}

/// Validates that a given block wasn't reverted. Given an instance of this class, we can call its
/// `validate` method and it will validate that the block's hash didn't change from the validator's
/// creation.
pub(crate) struct BlockNotRevertedValidator {
    block_number: BlockNumber,
    old_block_hash: BlockHash,
}

impl BlockNotRevertedValidator {
    pub fn new<Mode: TransactionKind>(
        block_number: BlockNumber,
        txn: &StorageTxn<'_, Mode>,
    ) -> Result<Self, ErrorObjectOwned> {
        let header = txn
            .get_block_header(block_number)
            .map_err(internal_server_error)?
            .ok_or_else(|| {
                ErrorObjectOwned::from(internal_server_error(StorageError::DBInconsistency {
                    msg: format!("Missing block header {block_number}"),
                }))
            })?;
        Ok(Self { block_number, old_block_hash: header.block_hash })
    }

    pub fn validate(self, storage_reader: &StorageReader) -> Result<(), ErrorObjectOwned> {
        let error = ErrorObjectOwned::from(internal_server_error(format!(
            "Block {} was reverted mid-execution.",
            self.block_number
        )));
        let txn = storage_reader.begin_ro_txn().map_err(internal_server_error)?;
        let new_block_hash = txn
            .get_block_header(self.block_number)
            .map_err(internal_server_error)?
            .ok_or(error.clone())?
            .block_hash;
        if new_block_hash == self.old_block_hash { Ok(()) } else { Err(error) }
    }
}
