use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use starknet_api::block::NonzeroGasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::TransactionHash;

use crate::errors::MempoolError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    // TODO(Ayelet): Consider removing this field as it is duplicated in Transaction.
    pub address: ContractAddress,
    pub nonce: Nonce,
}

impl std::fmt::Display for AccountState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let AccountState { address, nonce } = self;
        write!(f, "AccountState {{ address: {address}, nonce: {nonce} }}")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddTransactionArgs {
    pub tx: InternalRpcTransaction,
    pub account_state: AccountState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommitBlockArgs {
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub rejected_tx_hashes: HashSet<TransactionHash>,
}

pub type MempoolResult<T> = Result<T, MempoolError>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MempoolSnapshot {
    pub transactions: Vec<TransactionHash>,
    pub transaction_queue: TransactionQueueSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionQueueSnapshot {
    pub gas_price_threshold: NonzeroGasPrice,
    pub priority_queue: Vec<TransactionHash>,
    pub pending_queue: Vec<TransactionHash>,
}
