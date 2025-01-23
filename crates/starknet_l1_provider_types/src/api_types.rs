use papyrus_base_layer::L1Event;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;

use crate::provider_client::L1ProviderResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationStatus {
    AlreadyIncludedInPropsedBlock,
    AlreadyIncludedOnL2,
    ConsumedOnL1OrUnknown,
    Validated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1ProviderRequest {
    AddEvents(Vec<Event>),
    CommitBlock { l1_handler_tx_hashes: Vec<TransactionHash>, height: BlockNumber },
    GetTransactions { n_txs: usize, height: BlockNumber },
    Validate { tx_hash: TransactionHash, height: BlockNumber },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum L1ProviderResponse {
    AddEvents(L1ProviderResult<()>),
    CommitBlock(L1ProviderResult<()>),
    GetTransactions(L1ProviderResult<Vec<L1HandlerTransaction>>),
    Validate(L1ProviderResult<ValidationStatus>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Event {
    L1HandlerTransaction(L1HandlerTransaction),
    TransactionCanceled(L1Event),
    TransactionCancellationStarted(L1Event),
    TransactionConsumed(L1Event),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Propose,
    Validate,
}
