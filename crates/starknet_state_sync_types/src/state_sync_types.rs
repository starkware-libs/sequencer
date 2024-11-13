use std::collections::HashMap;

use starknet_api::block::BlockHash;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::{ContractClass, ThinStateDiff};
use starknet_api::transaction::TransactionHash;

use crate::errors::StateSyncError;

pub type StateSyncResult<T> = Result<T, StateSyncError>;

pub struct SyncBlock {
    pub block_hash: BlockHash,
    pub parent_block_hash: BlockHash,
    pub state_diff: ThinStateDiff,
    pub classes: HashMap<ClassHash, ContractClass>,
    // TODO: Consider removing deprecated classes.
    pub deprecated_classes: HashMap<ClassHash, DeprecatedContractClass>,
    pub transaction_hashes: Vec<TransactionHash>,
}
