use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::Transaction;

use crate::errors::MempoolError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountState {
    pub nonce: Nonce,
    // TODO: add balance field when needed.
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    // TODO(Ayelet): Consider removing this field as it is duplicated in ThinTransaction.
    pub sender_address: ContractAddress,
    pub state: AccountState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MempoolInput {
    pub tx: Transaction,
    pub account: Account,
}

pub type MempoolResult<T> = Result<T, MempoolError>;
