use serde::{Deserialize, Serialize};

use crate::executable_transaction::L1HandlerTransaction;
use crate::rpc_transaction::{InternalRpcTransaction, RpcTransaction};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum ExternalTransactionV3 {
    RpcTransaction(RpcTransaction),
    L1Handler(L1HandlerTransaction),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum InternalTransactionV3 {
    RpcTransaction(InternalRpcTransaction),
    L1Handler(L1HandlerTransaction),
}
