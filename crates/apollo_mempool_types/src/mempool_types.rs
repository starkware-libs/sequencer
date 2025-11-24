use std::collections::HashMap;

use apollo_infra::requests::LABEL_NAME_REQUEST_VARIANT;
use apollo_metrics::generate_permutation_labels;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::InternalRpcTransaction;
use starknet_api::transaction::fields::Tip;
use starknet_api::transaction::TransactionHash;
use strum::VariantNames;

use crate::communication::MempoolRequestLabelValue;
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

impl AddTransactionArgs {
    pub fn new(tx: InternalRpcTransaction, nonce: Nonce) -> Self {
        let address = tx.contract_address();
        Self { tx, account_state: AccountState { address, nonce } }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationArgs {
    pub address: ContractAddress,
    pub account_nonce: Nonce,
    pub tx_hash: TransactionHash,
    pub tx_nonce: Nonce,
    pub tip: Tip,
    pub max_l2_gas_price: GasPrice,
}

impl ValidationArgs {
    pub fn new(args: &AddTransactionArgs) -> Self {
        Self {
            address: args.tx.contract_address(),
            account_nonce: args.account_state.nonce,
            tx_hash: args.tx.tx_hash(),
            tx_nonce: args.tx.nonce(),
            tip: args.tx.tip(),
            max_l2_gas_price: args.tx.resource_bounds().l2_gas.max_price_per_unit,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CommitBlockArgs {
    pub address_to_nonce: HashMap<ContractAddress, Nonce>,
    pub rejected_tx_hashes: IndexSet<TransactionHash>,
}

pub type MempoolResult<T> = Result<T, MempoolError>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MempoolSnapshot {
    pub transactions: Vec<TransactionHash>,
    pub delayed_declares: Vec<TransactionHash>,
    pub transaction_queue: TransactionQueueSnapshot,
    pub mempool_state: MempoolStateSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionQueueSnapshot {
    pub gas_price_threshold: GasPrice,
    pub priority_queue: Vec<TransactionHash>,
    pub pending_queue: Vec<TransactionHash>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MempoolStateSnapshot {
    pub committed: HashMap<ContractAddress, Nonce>,
    pub staged: HashMap<ContractAddress, Nonce>,
}

generate_permutation_labels! {
    MEMPOOL_REQUEST_LABELS,
    (LABEL_NAME_REQUEST_VARIANT, MempoolRequestLabelValue),
}
