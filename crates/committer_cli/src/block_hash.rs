use serde::Deserialize;
use starknet_api::block::{BlockHeaderWithoutHash, StarknetVersion};
use starknet_api::block_hash::block_hash_calculator::{
    BlockHeaderCommitments,
    TransactionHashingData,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::ThinStateDiff;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BlockCommitmentsInput {
    pub transactions_data: Vec<TransactionHashingData>,
    pub state_diff: ThinStateDiff,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub starknet_version: StarknetVersion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct BlockHashInput {
    pub header: BlockHeaderWithoutHash,
    pub block_commitments: BlockHeaderCommitments,
}
