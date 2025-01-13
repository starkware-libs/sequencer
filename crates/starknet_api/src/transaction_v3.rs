use serde::{Deserialize, Serialize};

use crate::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use crate::{executable_transaction, transaction};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum ExternalTransactionV3 {
    RpcTransaction(RpcTransaction),
    L1Handler(transaction::L1HandlerTransaction),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum InternalTransactionV3 {
    RpcTransaction(InternalRpcTransaction),
    L1Handler(executable_transaction::L1HandlerTransaction),
}
