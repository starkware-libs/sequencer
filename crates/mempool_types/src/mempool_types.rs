use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::fields::TransactionHash;

use crate::errors::MempoolError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    // TODO(Ayelet): Consider removing this field as it is duplicated in Transaction.
    pub address: ContractAddress,
    pub nonce: Nonce,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddTransactionArgs {
    pub tx: Transaction,
    pub account_state: AccountState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommitBlockArgs {
    pub nonces: HashMap<ContractAddress, Nonce>,
    pub tx_hashes: HashSet<TransactionHash>,
}

pub type MempoolResult<T> = Result<T, MempoolError>;
