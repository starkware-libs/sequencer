use ::serde::{Deserialize, Serialize};

use crate::rpc_transaction::RpcTransaction;
use crate::transaction::{
    DeclareTransactionV3,
    DeployAccountTransactionV3,
    InvokeTransactionV3,
    // TODO(alonl): Ask Gilad if this is the right L1HandlerTransaction
    L1HandlerTransaction,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum InternalRpcTransaction {
    #[serde(rename = "DECLARE")]
    Declare(DeclareTransactionV3),
    #[serde(rename = "INVOKE")]
    Invoke(InvokeTransactionV3),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(DeployAccountTransactionV3),
}

pub enum ExternalTransaction {
    RpcTransaction(RpcTransaction),
    L1Handler(L1HandlerTransaction),
}

pub enum InternalTransaction {
    RpcTransaction(InternalRpcTransaction),
    L1Handler(L1HandlerTransaction),
}
