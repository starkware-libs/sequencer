use serde::{Deserialize, Serialize};
use starknet_api::block::GasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum MempoolError {
    #[error("Duplicate nonce, sender address: {address}, nonce: {:?}", nonce)]
    DuplicateNonce { address: ContractAddress, nonce: Nonce },
    #[error("Duplicate transaction, with hash: {tx_hash}")]
    DuplicateTransaction { tx_hash: TransactionHash },
    #[error("Transactions gas price {max_l2_gas_price} is below the threshold {threshold}.")]
    GasPriceTooLow { max_l2_gas_price: GasPrice, threshold: GasPrice },
    #[error("{0}")]
    NonceTooLarge(Nonce),
    #[error("Invalid transaction nonce. Expected: {account_nonce}, got: {tx_nonce}.")]
    NonceTooOld { address: ContractAddress, tx_nonce: Nonce, account_nonce: Nonce },
    #[error("Transaction with hash: {tx_hash} could not be sent using p2p client.")]
    P2pPropagatorClientError { tx_hash: TransactionHash },
    #[error("Transaction with hash: {tx_hash} not found")]
    TransactionNotFound { tx_hash: TransactionHash },
    #[error("Transaction rejected: mempool capacity exceeded.")]
    MempoolFull,
}
