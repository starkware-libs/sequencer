use serde::{Deserialize, Serialize};

use crate::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use crate::transaction::fields::TransactionSignature;
use crate::transaction::TransactionHash;
use crate::{executable_transaction, transaction};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum ConsensusTransaction {
    RpcTransaction(RpcTransaction),
    L1Handler(transaction::L1HandlerTransaction),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum InternalConsensusTransaction {
    RpcTransaction(InternalRpcTransaction),
    L1Handler(executable_transaction::L1HandlerTransaction),
}

impl InternalConsensusTransaction {
    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            Self::RpcTransaction(tx) => tx.tx_hash,
            Self::L1Handler(tx) => tx.tx_hash,
        }
    }

    /// Returns the transaction signature for commitment.
    /// Returns `None` for L1Handler transactions, as they do not have a signature.
    pub fn tx_signature_for_commitment(&self) -> Option<TransactionSignature> {
        match self {
            Self::RpcTransaction(tx) => Some(tx.signature()),
            Self::L1Handler(_) => None,
        }
    }
}
