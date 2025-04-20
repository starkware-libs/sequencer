// This file should contain the deprecated structs and the corresponding migration logic.
// Check file history for examples.

use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPrice, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use tracing::error;

use crate::db::serialization::{Migratable, StorageSerde, StorageSerdeError};
use crate::header::StorageBlockHeader;

impl Migratable for StorageBlockHeader {
    fn try_from_older_version(
        bytes: &mut impl std::io::Read,
        older_version: u8,
    ) -> Result<Self, StorageSerdeError> {
        // TODO: Once we have version 3, extract the code below to a function.
        const CURRENT_VERSION: u8 = 1;
        // match doesn't allow any calculations in patterns
        const PREV_VERSION: u8 = CURRENT_VERSION - 1;

        let prev_version_block_header = match older_version {
            PREV_VERSION => {
                StorageBlockHeaderV0::deserialize_from(bytes).ok_or(StorageSerdeError::Migration)
            }
            CURRENT_VERSION.. => {
                error!(
                    "Unable to migrate stored header from version {} to current version.",
                    older_version
                );
                Err(StorageSerdeError::Migration)
            }
        }?;
        Ok(prev_version_block_header.into())
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub(crate) struct StorageBlockHeaderV0 {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub l2_gas_consumed: GasAmount,
    pub next_l2_gas_price: u64,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub state_diff_commitment: Option<StateDiffCommitment>,
    pub transaction_commitment: Option<TransactionCommitment>,
    pub event_commitment: Option<EventCommitment>,
    pub receipt_commitment: Option<ReceiptCommitment>,
    pub state_diff_length: Option<usize>,
    pub n_transactions: usize,
    pub n_events: usize,
}

impl From<StorageBlockHeaderV0> for StorageBlockHeader {
    fn from(v0_header: StorageBlockHeaderV0) -> Self {
        Self {
            block_hash: v0_header.block_hash,
            parent_hash: v0_header.parent_hash,
            block_number: v0_header.block_number,
            l1_gas_price: v0_header.l1_gas_price,
            l1_data_gas_price: v0_header.l1_data_gas_price,
            l2_gas_price: v0_header.l2_gas_price,
            l2_gas_consumed: v0_header.l2_gas_consumed,
            next_l2_gas_price: GasPrice(v0_header.next_l2_gas_price as u128),
            state_root: v0_header.state_root,
            sequencer: v0_header.sequencer,
            timestamp: v0_header.timestamp,
            l1_da_mode: v0_header.l1_da_mode,
            state_diff_commitment: v0_header.state_diff_commitment,
            transaction_commitment: v0_header.transaction_commitment,
            event_commitment: v0_header.event_commitment,
            receipt_commitment: v0_header.receipt_commitment,
            state_diff_length: v0_header.state_diff_length,
            n_transactions: v0_header.n_transactions,
            n_events: v0_header.n_events,
        }
    }
}
