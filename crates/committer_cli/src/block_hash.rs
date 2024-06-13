use serde::Deserialize;
use starknet_api::{
    block_hash::block_hash_calculator::TransactionHashingData,
    data_availability::L1DataAvailabilityMode, state::ThinStateDiff,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct BlockCommitmentsInput {
    pub transactions_data: Vec<TransactionHashingData>,
    pub state_diff: ThinStateDiff,
    pub l1_da_mode: L1DataAvailabilityMode,
}
