use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MempoolError {
    #[error("Duplicate transaction, with hash: {tx_hash}")]
    DuplicateTransaction { tx_hash: TransactionHash },
    #[error("Transaction with hash: {tx_hash} not found")]
    TransactionNotFound { tx_hash: TransactionHash },
    // TODO(Mohammad): Consider using `StarknetApiError` once it implements `PartialEq`.
    #[error("Out of range.")]
    FeltOutOfRange,
}
