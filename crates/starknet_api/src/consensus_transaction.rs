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

    pub fn tx_signature_for_block_hash(&self) -> TransactionSignature {
        match self {
            Self::RpcTransaction(tx) => tx.signature(),
            // L1Handler transactions do not have a signature in their structure, but for block hash
            // calculation the signature is considered to be empty.
            Self::L1Handler(_) => TransactionSignature::default(),
        }
    }
}
