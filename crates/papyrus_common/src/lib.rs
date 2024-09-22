use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;

pub mod class_hash;
pub mod deprecated_class_abi;
pub mod metrics;
pub mod pending_classes;
pub mod state;
pub mod storage_query;

#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
}

pub(crate) fn usize_into_felt(u: usize) -> Felt {
    u128::try_from(u).expect("Expect at most 128 bits").into()
}
