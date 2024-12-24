use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::SequencerContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::errors::StateSyncError;

pub type StateSyncResult<T> = Result<T, StateSyncError>;

/// A block that came from the state sync.
/// Contains all the data needed to update the state of the system about this block.
///
/// Blocks that came from the state sync are trusted. Therefore, SyncBlock doesn't contain data
/// needed for verifying the block
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SyncBlock {
    pub state_diff: ThinStateDiff,
    // TODO: decide if we want block hash, parent block hash and full classes here.
    pub transaction_hashes: Vec<TransactionHash>,

    pub block_number: BlockNumber,
    pub timestamp: BlockTimestamp,
    pub sequencer: SequencerContractAddress,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub l1_da_mode: L1DataAvailabilityMode,
}
