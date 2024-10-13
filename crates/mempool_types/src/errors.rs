use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::fields::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum MempoolError {
    #[error("Duplicate transaction, sender address: {address}, nonce: {:?}", nonce)]
    DuplicateNonce { address: ContractAddress, nonce: Nonce },
    #[error("Duplicate transaction, with hash: {tx_hash}")]
    DuplicateTransaction { tx_hash: TransactionHash },
    #[error("Transaction with hash: {tx_hash} not found")]
    TransactionNotFound { tx_hash: TransactionHash },
    // TODO(Mohammad): Consider using `StarknetApiError` once it implements `PartialEq`.
    #[error("Out of range.")]
    FeltOutOfRange,
}
