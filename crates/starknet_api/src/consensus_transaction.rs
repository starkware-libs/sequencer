use serde::{Deserialize, Serialize};

use crate::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
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
            InternalConsensusTransaction::RpcTransaction(tx) => tx.tx_hash,
            InternalConsensusTransaction::L1Handler(tx) => tx.tx_hash,
        }
    }
}
