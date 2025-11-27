use serde::Deserialize;
use starknet_api::block::{BlockHash, BlockHeaderWithoutHash, StarknetVersion};
use starknet_api::block_hash::block_hash_calculator::{
    BlockHeaderCommitments,
    PartialBlockHashComponents,
    TransactionHashingData,
};
use starknet_api::core::GlobalRoot;
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

impl BlockHashInput {
    pub fn to_final_block_hash_components(
        self,
    ) -> (PartialBlockHashComponents, GlobalRoot, BlockHash) {
        (
            PartialBlockHashComponents {
                starknet_version: self.header.starknet_version,
                header_commitments: self.block_commitments,
                block_number: self.header.block_number,
                l1_gas_price: self.header.l1_gas_price,
                l1_data_gas_price: self.header.l1_data_gas_price,
                l2_gas_price: self.header.l2_gas_price,
                sequencer: self.header.sequencer,
                timestamp: self.header.timestamp,
            },
            self.header.state_root,
            self.header.parent_hash,
        )
    }
}
