use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum MempoolError {
    #[error("Duplicate transaction, of hash: {tx_hash}")]
    DuplicateTransaction { tx_hash: TransactionHash },
}
