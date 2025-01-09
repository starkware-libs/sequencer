use ::serde::{Deserialize, Serialize};

use crate::executable_transaction::L1HandlerTransaction;
use crate::transaction::{DeclareTransactionV3, DeployAccountTransactionV3, InvokeTransactionV3};

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub enum InternalTransaction {
    RpcTransaction(InternalRpcTransaction),
    L1Handler(L1HandlerTransaction),
}
